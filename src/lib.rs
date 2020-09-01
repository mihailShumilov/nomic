#![feature(proc_macro_hygiene, decl_macro)]
#![feature(generic_associated_types)]
#![feature(negative_impls)]
#![feature(trait_alias)]

#[macro_use]
extern crate rocket;
pub mod chain;
pub mod cli;
pub mod core;
pub mod relayer;
pub mod signatory;
pub mod worker;

pub use failure::Error;
pub type Result<T> = std::result::Result<T, Error>;
