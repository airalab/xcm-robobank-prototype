use crate::{mock::*, Error};
use frame_support::{assert_noop, assert_ok};
use frame_system::ensure_signed;

fn account(id: u64) -> AccountId {
    let mut b = [0_u8; 32];
    let id = id.to_ne_bytes();

    unsafe { std::ptr::copy_nonoverlapping(id.as_ptr(), b.as_mut_ptr(), id.len()) };
    b.into()
}

#[test]
fn it_works_for_default_value() {
    new_test_ext().execute_with(|| {
        // Dispatch a signed extrinsic.
        assert_ok!(TemplateModule::do_something(Origin::signed(account(1)), 42));
        // Read pallet storage and assert an expected result.
        assert_eq!(TemplateModule::something(), Some(42));
    });
}

#[test]
fn correct_error_for_none_value() {
    new_test_ext().execute_with(|| {
        // Ensure the expected error is thrown when no value is present.
        assert_noop!(
            TemplateModule::cause_error(Origin::signed(account(1))),
            Error::<Test>::NoneValue
        );
    });
}

#[test]
fn hrmp_origin_converter() {
    new_test_ext().execute_with(|| {
        for paraid in &[100, 200, 1000] {
            let origin = TemplateModule::origin_convert(*paraid).unwrap();
            println!("{} origin: {:?}", *paraid, origin);
            let account: AccountId = ensure_signed(origin).unwrap();
            println!("{} account: {:?}", *paraid, account);
        }
    });
}
