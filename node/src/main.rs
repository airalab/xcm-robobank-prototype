#![warn(unused_extern_crates)]

#[cfg(all(feature = "service", feature = "client"))]
compile_error!("'service' and 'client' features mutually exclusive and cannot be enabled together");
#[cfg(not(any(feature = "service", feature = "client")))]
compile_error!("one of the features 'service' or  'client' must be provided");

mod chain_spec;
#[macro_use]
mod service;
mod cli;
mod command;

fn main() -> sc_cli::Result<()> {
    command::run()
}
