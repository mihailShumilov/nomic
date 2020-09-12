use crate::core::market::order_book::SATOSHIS_PER_BITCOIN;
use crate::core::market::{Direction, Order};
use crate::Result;
use failure::bail;
use orga::encoding::{self as ed, Decode, Encode};

const PRICE_PRECISION: u64 = 100_000_000;

pub const LEVERAGE_PRECISION: u16 = 100;
pub const MAX_LEVERAGE: u16 = 100 * LEVERAGE_PRECISION;

#[derive(Clone, Debug, Encode, Decode, Eq, PartialEq, Copy)]
pub struct Account {
    pub nonce: u64,
    pub balance: u64,

    // Funds are moved from balance into order_margin on order creation
    pub order_margin: u64,
    pub max_bid_margin: u64,
    pub max_ask_margin: u64,

    // Position fields
    pub size: u64,
    pub side: Direction,
    // Price is cents/bitcoin
    pub entry_price: u64,
    pub position_margin: u64,
    pub desired_leverage: u16,
}

impl Default for Account {
    fn default() -> Self {
        Self {
            nonce: 0,
            balance: 0,
            order_margin: 0,
            max_bid_margin: 0,
            max_ask_margin: 0,
            size: 0,
            side: Direction::default(),
            entry_price: 0,
            position_margin: 0,
            desired_leverage: LEVERAGE_PRECISION,
        }
    }
}

impl Account {
    pub fn new(balance: u64) -> Self {
        Self {
            balance,
            ..Self::default()
        }
    }

    #[cfg(test)]
    pub(crate) fn with_nonce(mut self, nonce: u64) -> Self {
        self.nonce = nonce;
        self
    }

    pub fn value(&self) -> u64 {
        if self.size == 0 {
            0
        } else {
            self.size * SATOSHIS_PER_BITCOIN / self.entry_price
        }
    }

    fn divide_by_leverage(&self, n: u64) -> u64 {
        n * PRICE_PRECISION * (LEVERAGE_PRECISION as u64)
            / (self.desired_leverage as u64)
            / PRICE_PRECISION
    }

    pub fn create_order(&mut self, side: Direction, order: Order) -> Result<()> {
        match side {
            Direction::Long => self.max_bid_margin += order.cost(),
            Direction::Short => self.max_ask_margin += order.cost(),
        };

        let unlocked = self.update_order_margin()?;
        debug_assert_eq!(
            unlocked, 0,
            "Funds should not be unlocked when creating an order"
        );

        Ok(())
    }

    pub fn fill_order(
        &mut self,
        maker_side: Direction,
        maker_order: Order,
        is_own_order: bool,
    ) -> Result<()> {
        if self.size == 0 {
            self.side = if is_own_order {
                maker_side
            } else {
                maker_side.other()
            };
        }
        let prev_self = *self;
        let position_increasing = (maker_side == self.side) && is_own_order;
        self.update_entry_price(maker_order, position_increasing);
        self.add_to_position(
            maker_order.size,
            if is_own_order {
                maker_side
            } else {
                maker_side.other()
            },
        );

        // fund margin from `order_margin`, or return locked funds to balance
        if is_own_order {
            match maker_side {
                Direction::Long => self.max_bid_margin -= maker_order.cost(),
                Direction::Short => self.max_ask_margin -= maker_order.cost(),
            }

            let new_margin = self.divide_by_leverage(self.value());
            let margin_increasing = new_margin > self.position_margin;

            let unlocked = self.update_order_margin()?;
            match margin_increasing {
                true => self.position_margin += unlocked,
                false => self.balance += unlocked,
            }
        }

        self.balance += self.update_position_margin()?;

        self.add_pnl(prev_self, maker_order.price);

        Ok(())
    }

    fn update_order_margin(&mut self) -> Result<u64> {
        let mut max_bid_margin = self.divide_by_leverage(self.max_bid_margin);
        let mut max_ask_margin = self.divide_by_leverage(self.max_ask_margin);
        match self.side {
            Direction::Long => {
                max_ask_margin = max_ask_margin.saturating_sub(self.position_margin);
            }
            Direction::Short => {
                max_bid_margin = max_bid_margin.saturating_sub(self.position_margin);
            }
        };

        let new_om = max_bid_margin + max_ask_margin;

        if new_om > self.order_margin {
            // if new_om increased, we're pulling money from our balance (and
            // erroring if there's not enough)
            let to_lock = new_om - self.order_margin;
            if self.balance < to_lock {
                bail!("Insufficient funds");
            }
            self.balance -= to_lock;
            self.order_margin += to_lock;
            Ok(0)
        } else {
            // if new_om decreased, we're returning funds to somewhere else
            let to_unlock = self.order_margin - new_om;
            self.order_margin -= to_unlock;
            Ok(to_unlock)
        }
    }

    fn update_position_margin(&mut self) -> Result<u64> {
        let new_margin = self.divide_by_leverage(self.value());
        let margin_increasing = new_margin > self.position_margin;
        if margin_increasing {
            let margin_change = new_margin - self.position_margin;
            if self.balance < margin_change {
                bail!("Insufficient funds");
            }
            self.balance -= margin_change;
            self.position_margin += margin_change;
            Ok(0)
        } else {
            let margin_change = self.position_margin - new_margin;
            self.position_margin -= margin_change;
            Ok(margin_change)
        }
    }

    fn add_to_position(&mut self, size: u64, side: Direction) {
        if side == self.side {
            // increase position
            self.size += size;
        } else if size > self.size {
            // reverse position
            self.size = size - self.size;
            self.side = side;
        } else {
            // reduce position without reversing
            self.size -= size;
        }
    }

    fn update_entry_price(&mut self, order: Order, position_increasing: bool) {
        let position_reversing = !position_increasing && order.size > self.size;
        self.entry_price = match (position_increasing, position_reversing) {
            // Increasing position
            (true, _) => {
                let new_size = order.size + self.size;
                let ratio = PRICE_PRECISION * order.size / new_size;
                (ratio * order.price + (PRICE_PRECISION - ratio) * self.entry_price)
                    / PRICE_PRECISION
            }
            // Decreasing position size on same side
            (false, false) => {
                // Entry price doesn't change
                self.entry_price
            }
            // Reversing position
            (false, true) => {
                // Entry price is the price of the order that caused our position to reverse
                order.price
            }
        }
    }

    fn add_pnl(&mut self, prev_self: Account, price: u64) {
        let position_reversed = self.side != prev_self.side;
        let position_increased = self.size > prev_self.size;
        let amount_reduced = match (position_reversed, position_increased) {
            (true, _) => prev_self.size,
            (false, true) => 0,
            (false, false) => prev_self.size - self.size,
        };

        if amount_reduced == 0 {
            return;
        }

        let amount_reduced_sats = amount_reduced * SATOSHIS_PER_BITCOIN;

        let (to_pay, to_receive, gained) = match prev_self.side {
            Direction::Long => (
                amount_reduced_sats / price,
                amount_reduced_sats / prev_self.entry_price,
                price > prev_self.entry_price,
            ),
            Direction::Short => (
                amount_reduced_sats / prev_self.entry_price,
                amount_reduced_sats / price,
                price < prev_self.entry_price,
            ),
        };

        if gained {
            let profit = to_receive - to_pay;
            self.balance += profit;
        } else {
            let loss = to_pay - to_receive;
            self.balance -= loss;
        }
    }

    pub fn adjust_leverage(&mut self, leverage: u16) -> Result<()> {
        debug_assert!(leverage >= LEVERAGE_PRECISION as u16);
        debug_assert!(leverage <= MAX_LEVERAGE);

        self.desired_leverage = leverage;

        self.balance += self.update_position_margin()?;
        self.balance += self.update_order_margin()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::market::{Direction, Order};

    #[test]
    fn new_account() {
        let account = Account::new(1234);
        assert_eq!(account.balance, 1234);
    }

    #[test]
    fn create_then_fill_orders() {
        fn order(price: u64, size: u64) -> Order {
            Order {
                creator: [2; 33],
                height: 42,
                price,
                size,
            }
        }

        let mut account = Account::new(100_000_000);
        assert_eq!(account.order_margin, 0);
        assert_eq!(account.max_ask_margin, 0);
        assert_eq!(account.max_bid_margin, 0);
        assert_eq!(account.balance, 100_000_000);
        assert_eq!(account.position_margin, 0);

        account
            .create_order(Direction::Long, order(100, 1))
            .unwrap();
        assert_eq!(account.order_margin, 1_000_000);
        assert_eq!(account.max_ask_margin, 0);
        assert_eq!(account.max_bid_margin, 1_000_000);
        assert_eq!(account.balance, 99_000_000);
        assert_eq!(account.position_margin, 0);

        account
            .create_order(Direction::Short, order(200, 1))
            .unwrap();
        assert_eq!(account.order_margin, 1_500_000);
        assert_eq!(account.max_ask_margin, 500_000);
        assert_eq!(account.max_bid_margin, 1_000_000);
        assert_eq!(account.balance, 98_500_000);
        assert_eq!(account.position_margin, 0);

        account
            .create_order(Direction::Long, order(100, 1))
            .unwrap();
        assert_eq!(account.order_margin, 2_500_000);
        assert_eq!(account.max_ask_margin, 500_000);
        assert_eq!(account.max_bid_margin, 2_000_000);
        assert_eq!(account.balance, 97_500_000);
        assert_eq!(account.position_margin, 0);

        account
            .fill_order(Direction::Long, order(100, 1), true)
            .unwrap();
        assert_eq!(account.order_margin, 1_500_000);
        assert_eq!(account.max_ask_margin, 500_000);
        assert_eq!(account.max_bid_margin, 1_000_000);
        assert_eq!(account.balance, 97_500_000);
        assert_eq!(account.position_margin, 1_000_000);
        assert_eq!(account.entry_price, 100);

        account
            .fill_order(Direction::Short, order(200, 1), true)
            .unwrap();
        assert_eq!(account.order_margin, 1_000_000);
        assert_eq!(account.max_ask_margin, 0);
        assert_eq!(account.max_bid_margin, 1_000_000);
        assert_eq!(account.size, 0);
        assert_eq!(account.balance, 99_500_000);
        assert_eq!(account.position_margin, 0);
        assert_eq!(account.entry_price, 100);

        account
            .adjust_leverage(account.desired_leverage * 2)
            .unwrap();
        assert_eq!(account.order_margin, 500_000);
        assert_eq!(account.max_ask_margin, 0);
        assert_eq!(account.max_bid_margin, 1_000_000);
        assert_eq!(account.size, 0);
        assert_eq!(account.balance, 100_000_000);
        assert_eq!(account.position_margin, 0);
        assert_eq!(account.entry_price, 100);

        account
            .fill_order(Direction::Long, order(100, 1), true)
            .unwrap();
        assert_eq!(account.order_margin, 0);
        assert_eq!(account.max_ask_margin, 0);
        assert_eq!(account.max_bid_margin, 0);
        assert_eq!(account.balance, 100_000_000);
        assert_eq!(account.position_margin, 500_000);
        assert_eq!(account.entry_price, 100);

        account
            .adjust_leverage(account.desired_leverage * 2)
            .unwrap();
        assert_eq!(account.order_margin, 0);
        assert_eq!(account.max_ask_margin, 0);
        assert_eq!(account.max_bid_margin, 0);
        assert_eq!(account.balance, 100_250_000);
        assert_eq!(account.position_margin, 250_000);
        assert_eq!(account.entry_price, 100);

        account
            .fill_order(Direction::Long, order(200, 1), false)
            .unwrap();
        assert_eq!(account.order_margin, 0);
        assert_eq!(account.max_ask_margin, 0);
        assert_eq!(account.max_bid_margin, 0);
        assert_eq!(account.balance, 101_000_000);
        assert_eq!(account.position_margin, 0);
        assert_eq!(account.entry_price, 100);
    }

    #[test]
    fn update_entry_price() {
        let mut account = Account::new(100_000_000);
        assert_eq!(account.entry_price, 0);

        // Increase position from 0
        account.update_entry_price(
            Order {
                creator: [2; 33],
                height: 42,
                size: 100,
                price: 1000,
            },
            true,
        );
        account.size = 100;
        assert_eq!(account.entry_price, 1000);

        // Increase position further
        account.update_entry_price(
            Order {
                creator: [2; 33],
                height: 42,
                size: 100,
                price: 2000,
            },
            true,
        );
        account.size = 200;
        assert_eq!(account.entry_price, 1500);

        // Decrease position, same side
        account.update_entry_price(
            Order {
                creator: [2; 33],
                height: 42,
                size: 100,
                price: 500,
            },
            false,
        );
        account.size = 100;
        assert_eq!(account.entry_price, 1500);

        // Reverse
        account.update_entry_price(
            Order {
                creator: [2; 33],
                height: 42,
                size: 400,
                price: 3000,
            },
            false,
        );
        account.size = 300;
        assert_eq!(account.entry_price, 3000);
    }

    #[test]
    fn add_pnl_long_profit() {
        let mut account = Account::new(0);
        account.size = 500_00;
        account.side = Direction::Long;

        let mut prev_account = Account::new(0);
        prev_account.entry_price = 1000_00;
        prev_account.size = 1000_00;
        prev_account.side = Direction::Long;

        account.add_pnl(prev_account, 2000_00);
        assert_eq!(account.balance, 25_000_000);
    }

    #[test]
    fn add_pnl_short_loss() {
        let mut account = Account::new(25_000_000);
        account.size = 500_00;
        account.side = Direction::Short;

        let mut prev_account = Account::new(0);
        prev_account.entry_price = 1000_00;
        prev_account.size = 1000_00;
        prev_account.side = Direction::Short;

        account.add_pnl(prev_account, 2000_00);
        assert_eq!(account.balance, 0);
    }

    #[test]
    fn add_pnl_long_loss() {
        let mut account = Account::new(25_000_000);
        account.size = 500_00;
        account.side = Direction::Long;

        let mut prev_account = Account::new(0);
        prev_account.entry_price = 2000_00;
        prev_account.size = 1000_00;
        prev_account.side = Direction::Long;

        account.add_pnl(prev_account, 1000_00);
        assert_eq!(account.balance, 0);
    }

    #[test]
    fn add_pnl_short_profit() {
        let mut account = Account::new(0);
        account.size = 500_00;
        account.side = Direction::Short;

        let mut prev_account = Account::new(0);
        prev_account.entry_price = 1000_00;
        prev_account.size = 1000_00;
        prev_account.side = Direction::Short;

        account.add_pnl(prev_account, 500_00);
        assert_eq!(account.balance, 50_000_000);
    }

    #[test]
    fn add_to_position() {
        let mut account = Account::new(0);
        account.add_to_position(100, Direction::Long);
        assert_eq!(account.size, 100);
        assert_eq!(account.side, Direction::Long);

        account.add_to_position(100, Direction::Long);
        assert_eq!(account.size, 200);
        assert_eq!(account.side, Direction::Long);

        account.add_to_position(50, Direction::Short);
        assert_eq!(account.size, 150);
        assert_eq!(account.side, Direction::Long);

        account.add_to_position(200, Direction::Short);
        assert_eq!(account.size, 50);
        assert_eq!(account.side, Direction::Short);
    }

    #[test]
    fn leverage_division() {
        let n: u64 = 1000;
        let mut account = Account::new(1_000_000);
        assert_eq!(account.divide_by_leverage(n), n);

        account.desired_leverage = 2_00;
        assert_eq!(account.divide_by_leverage(n), 500);

        account.desired_leverage = 3_00;
        assert_eq!(account.divide_by_leverage(n), 333);

        account.desired_leverage = 100_00;
        assert_eq!(account.divide_by_leverage(n), 10);
    }
}
