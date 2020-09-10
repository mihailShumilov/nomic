use crate::core::market::order_book::SATOSHIS_PER_BITCOIN;
use crate::core::market::{Direction, Order};
use crate::Result;
use failure::bail;
use orga::encoding::{self as ed, Decode, Encode};
use std::cmp::max;

const PRICE_PRECISION: u64 = 100_000_000;
const LEVERAGE_PRECISION: u64 = 100;

#[derive(Clone, Debug, Default, Encode, Decode, Eq, PartialEq, Copy)]
pub struct Account {
    pub nonce: u64,
    pub balance: u64,

    // Funds are moved from balance into locked_in_orders on order creation
    pub locked_in_orders: u64,
    pub max_bid_margin: u64,
    pub max_ask_margin: u64,

    // Position fields
    pub size: u64,
    pub side: Direction,
    // Price is cents/bitcoin
    pub entry_price: u64,
    pub margin: u64,
    pub desired_leverage: u16,
}

impl Account {
    pub fn new(balance: u64) -> Self {
        Self {
            balance,
            desired_leverage: LEVERAGE_PRECISION as u16,
            ..Self::default()
        }
    }

    #[cfg(test)]
    pub(crate) fn with_nonce(mut self, nonce: u64) -> Self {
        self.nonce = nonce;
        self
    }

    pub fn value(&self) -> u64 {
        self.size * SATOSHIS_PER_BITCOIN / self.entry_price
    }

    fn divide_by_leverage(&self, n: u64) -> u64 {
        n * PRICE_PRECISION * LEVERAGE_PRECISION / self.desired_leverage as u64 / PRICE_PRECISION
    }

    pub fn create_order(&mut self, side: Direction, order: Order) -> Result<()> {
        let cost = self.divide_by_leverage(order.cost());
        match side {
            Direction::Long => self.max_bid_margin += cost,
            Direction::Short => self.max_ask_margin += cost,
        };

        let unlocked = self.update_locked_in_orders()?;
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

        // fund margin from `locked_in_orders`, or return locked funds to balance
        if is_own_order {
            let cost = self.divide_by_leverage(maker_order.cost());
            match maker_side {
                Direction::Long => self.max_bid_margin -= cost,
                Direction::Short => self.max_ask_margin -= cost,
            }

            let new_margin = self.value();
            let margin_increasing = new_margin > self.margin;

            let unlocked = self.update_locked_in_orders()?;
            match margin_increasing {
                true => self.margin += unlocked,
                false => debug_assert_eq!(
                    unlocked, 0,
                    "Funds should not be unlocked when reducing margin"
                ),
            }
        }

        // move funds from balance to margin or vice versa. makers will already
        // have their margin funded from `locked_in_orders` in the section
        // above.
        let new_margin = self.divide_by_leverage(self.value());
        let margin_increasing = new_margin > self.margin;
        if margin_increasing {
            let margin_change = new_margin - self.margin;
            if self.balance < margin_change {
                bail!("Insufficient funds");
            }
            self.balance -= margin_change;
            self.margin += margin_change;
        } else {
            let margin_change = self.margin - new_margin;
            self.balance += margin_change;
            self.margin -= margin_change;
        }

        self.add_pnl(prev_self, maker_order.price);

        Ok(())
    }

    fn update_locked_in_orders(&mut self) -> Result<u64> {
        let mut max_bid_margin = self.max_bid_margin;
        let mut max_ask_margin = self.max_ask_margin;
        match self.side {
            Direction::Long => {
                max_ask_margin = max_ask_margin.saturating_sub(self.margin);
            }
            Direction::Short => {
                max_bid_margin = max_bid_margin.saturating_sub(self.margin);
            }
        };

        let new_lio = max(max_bid_margin, max_ask_margin);

        if new_lio > self.locked_in_orders {
            // if new_lio increased, we're pulling money from our balance (and
            // erroring if there's not enough)
            let to_lock = new_lio - self.locked_in_orders;
            if self.balance < to_lock {
                bail!("Insufficient funds");
            }
            self.balance -= to_lock;
            self.locked_in_orders += to_lock;
            Ok(0)
        } else {
            // if new_lio decreased, we're returning funds to somewhere else
            let to_unlock = self.locked_in_orders - new_lio;
            self.locked_in_orders -= to_unlock;
            Ok(to_unlock)
        }

        // (price is 10 sats per cent)
        // position: long 150 cents, margin = 1500 sats
        // max_bid_margin, max_ask_margin = 0
        // lio = 0

        // action: open long order for 50 cents (margin 500 sats)
        // max_bid_margin = 500
        // lio = max(max_bid_margin - short ? margin : 0, max_ask_margin - long ? margin : 0) = 500
        // balance -= 500
        // action: open short order for 25 cents (margin 250 sats)
        // max_bid_margin = 500
        // max_ask_margin = 250
        // lio = 500
        // action: short order fills (25 cents)
        // max_bid_margin = 500
        // max_ask_margin = 0
        // lio = max(500 - 0, 0 - 1500)

        // alternately...
        // action: open short order for 50 cents (margin 500 sats)
        // max_ask_margin = 500
        // lio = 0
        // action: open long order for 25 cents (margin 250 sats)
        // max_ask_margin = 500
        // max_bid_margin = 250
        // lio = 250
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
    fn locked_in_orders() {
        let mut account = Account::new(100_000_000);
        assert_eq!(account.locked_in_orders, 0);
        assert_eq!(account.max_ask_margin, 0);
        assert_eq!(account.max_bid_margin, 0);
        assert_eq!(account.balance, 100_000_000);
        assert_eq!(account.margin, 0);

        account
            .create_order(
                Direction::Long,
                Order {
                    creator: [2; 33],
                    height: 42,
                    price: 100,
                    size: 1,
                },
            )
            .unwrap();
        assert_eq!(account.locked_in_orders, 1_000_000);
        assert_eq!(account.max_ask_margin, 0);
        assert_eq!(account.max_bid_margin, 1_000_000);
        assert_eq!(account.balance, 99_000_000);
        assert_eq!(account.margin, 0);

        account
            .create_order(
                Direction::Short,
                Order {
                    creator: [2; 33],
                    height: 42,
                    price: 200,
                    size: 1,
                },
            )
            .unwrap();
        assert_eq!(account.locked_in_orders, 1_000_000);
        assert_eq!(account.max_ask_margin, 500_000);
        assert_eq!(account.max_bid_margin, 1_000_000);
        assert_eq!(account.balance, 99_000_000);
        assert_eq!(account.margin, 0);

        account
            .create_order(
                Direction::Long,
                Order {
                    creator: [2; 33],
                    height: 42,
                    price: 100,
                    size: 1,
                },
            )
            .unwrap();
        assert_eq!(account.locked_in_orders, 2_000_000);
        assert_eq!(account.max_ask_margin, 500_000);
        assert_eq!(account.max_bid_margin, 2_000_000);
        assert_eq!(account.balance, 98_000_000);
        assert_eq!(account.margin, 0);

        account
            .fill_order(
                Direction::Long,
                Order {
                    creator: [2; 33],
                    height: 42,
                    price: 100,
                    size: 1,
                },
                true,
            )
            .unwrap();
        assert_eq!(account.locked_in_orders, 1_000_000);
        assert_eq!(account.max_ask_margin, 500_000);
        assert_eq!(account.max_bid_margin, 1_000_000);
        assert_eq!(account.balance, 98_000_000);
        assert_eq!(account.margin, 1_000_000);

        account
            .fill_order(
                Direction::Short,
                Order {
                    creator: [2; 33],
                    height: 42,
                    price: 200,
                    size: 1,
                },
                true,
            )
            .unwrap();
        assert_eq!(account.locked_in_orders, 1_000_000);
        assert_eq!(account.max_ask_margin, 0);
        assert_eq!(account.max_bid_margin, 1_000_000);
        assert_eq!(account.balance, 98_500_000);
        assert_eq!(account.margin, 500_000);
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
