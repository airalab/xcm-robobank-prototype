#![cfg_attr(not(feature = "std"), no_std)]

use polkadot_parachain::primitives::{Id, Sibling};
use sp_core::crypto::AccountId32;
use sp_core::crypto::Ss58Codec;
use sp_core::TypeId;
use sp_runtime::codec::{Decode, Encode};
use sp_runtime::traits::AccountIdConversion;
use std::env::args;
use std::str::from_utf8_unchecked;

fn print_paraid<T: TypeId + From<u32> + Encode + Decode>(paraid: u32) {
    let account: AccountId32 = T::from(paraid).into_account();

    println!(
        "{} {}\naddress {}\n{:02x?}\n",
        unsafe { from_utf8_unchecked(T::TYPE_ID.as_ref()) },
        paraid,
        account.to_ss58check(),
        <AccountId32 as AsRef<[u8]>>::as_ref(&account)
    );
}

fn main() {
    println!("Parachain address");

    let mut param = args();
    let program_name = param.next().unwrap();
    if let Some(id) = param.next() {
        if id.eq("--help") {
            println!("{} [parachain-id]", program_name);
        } else {
            let id = id.parse::<u32>().expect("parachain id");
            print_paraid::<Id>(id);
            print_paraid::<Sibling>(id);
        }
    } else {
        for paraid in &[100_u32, 200, 300] {
            print_paraid::<Id>(*paraid);
            print_paraid::<Sibling>(*paraid);
        }
    }
}
