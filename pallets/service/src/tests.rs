use frame_support::dispatch::{DispatchError, DispatchResult};
use frame_support::traits::Currency;
use frame_support::{assert_err, assert_noop, assert_ok};
use frame_system::{ensure_signed, RawOrigin};

use crate::{mock::*, DeviceProfile, DeviceState, Error};

const DEV1: u64 = 100;
const DEV2: u64 = 101;
const DEV3: u64 = 102; // zero balance

const CL1: u64 = 200;
const CL2: u64 = 201;
const CL3: u64 = 203; // zero balance

const PENALTY: Balance = 10_000;
const DEFAULT_WCD: Moment = 1000;
const DEFAULT_FEE: Balance = 100_000;

type Moment = <Test as pallet_timestamp::Config>::Moment;
type OutOrder = crate::OrderBaseOf<Test>;
type Order = crate::OrderOf<Test>;

fn aux_register_device(id: u64, onoff: bool) -> Option<DeviceProfile<Test>> {
    ServiceModule::register(Origin::signed(account(id)), PENALTY, DEFAULT_WCD, onoff).ok()?;
    ServiceModule::devices(account(id))
}

fn aux_order(dev_id: u64, client: u64, fee: Balance, wcd: Moment) -> DispatchResult {
    let until = crate::Timestamp::<Test>::now() + wcd;

    ServiceModule::order(
        Origin::signed(account(client)),
        OutOrder {
            until,
            data: Vec::new(),
            fee,
            device: account(dev_id),
        },
    )
}

fn aux_free_balance(id: u64) -> Balance {
    Balances::free_balance(&account(id))
}

fn aux_total_balance(id: u64) -> Balance {
    Balances::total_balance(&account(id))
}

fn aux_init_order(dev_id: u64, client: u64, wcd: Moment) -> Result<Order, DispatchError> {
    let until = crate::Timestamp::<Test>::now() + wcd;
    let dev_id = account(dev_id);

    ServiceModule::register(Origin::signed(dev_id.clone()), PENALTY, DEFAULT_WCD, true)?;

    ServiceModule::order(
        Origin::signed(account(client)),
        OutOrder {
            until,
            data: Vec::new(),
            fee: DEFAULT_FEE,
            device: dev_id.clone(),
        },
    )?;

    ServiceModule::orders(&dev_id).ok_or(Error::<Test>::NoOrder.into())
}

macro_rules! assert_some {
    ( $x:expr $(,)? ) => {{
        let is = $x;
        match is {
            Some(v) => v,
            _ => {
                assert!(false, "Expected Some(_). Got {:#?}", is);
                panic!("");
            }
        }
    }};
}

#[test]
fn register_device() {
    new_test_ext().execute_with(|| {
        assert_ok!(ServiceModule::register(
            Origin::signed(account(DEV1)),
            PENALTY,
            DEFAULT_WCD,
            true
        ),);
        let dev = ServiceModule::devices(account(DEV1));

        let dev = assert_some!(dev);
        //let dev = dev.unwrap();

        assert_eq!(dev.state, DeviceState::Ready);
        assert_eq!(dev.wcd, 1000_u64);
    });
}

#[test]
fn register_device_then_on() {
    new_test_ext().execute_with(|| {
        let dev = aux_register_device(DEV1, false);
        let dev = assert_some!(dev);

        assert_eq!(dev.state, DeviceState::Off);

        assert_ok!(ServiceModule::set_state(
            Origin::signed(account(DEV1)),
            true
        ));
        let dev = ServiceModule::devices(account(DEV1)).unwrap();

        assert_eq!(dev.state, DeviceState::Ready);
    });
}

#[test]
fn order_when_off() {
    new_test_ext().execute_with(|| {
        assert_err!(
            aux_order(DEV1, CL1, DEFAULT_FEE, DEFAULT_WCD),
            Error::<Test>::NoDevice
        );
        aux_register_device(DEV1, false);
        assert_err!(
            aux_order(DEV1, CL1, DEFAULT_FEE, DEFAULT_WCD),
            Error::<Test>::IllegalState
        );
        // amend
        assert_ok!(ServiceModule::set_state(
            Origin::signed(account(DEV1)),
            true
        ));
        assert_ok!(aux_order(DEV1, CL1, DEFAULT_FEE, DEFAULT_WCD));
    });
}

#[test]
fn order_when_busy() {
    new_test_ext().execute_with(|| {
        let _dev1 = assert_some!(aux_register_device(DEV1, true));
        let _dev2 = assert_some!(aux_register_device(DEV2, true));

        assert_ok!(aux_order(DEV1, CL1, DEFAULT_FEE, DEFAULT_WCD));
        let dev1 = assert_some!(ServiceModule::devices(account(DEV1)));

        assert_eq!(dev1.state, DeviceState::Busy);
        assert_err!(
            aux_order(DEV1, CL2, DEFAULT_FEE, DEFAULT_WCD),
            Error::<Test>::IllegalState
        );
        assert_ok!(aux_order(DEV2, CL2, DEFAULT_FEE, DEFAULT_WCD));
    });
}

#[test]
fn order_rush() {
    new_test_ext().execute_with(|| {
        let _dev1 = assert_some!(aux_register_device(DEV1, true));
        assert_err!(
            aux_order(DEV1, CL1, DEFAULT_FEE, DEFAULT_WCD - 1),
            Error::<Test>::BadOrderDetails
        );
    });
}

#[test]
fn order_without_collateral() {
    new_test_ext().execute_with(|| {
        let _dev1 = assert_some!(aux_register_device(DEV1, true));
        assert!(aux_free_balance(DEV1) > PENALTY);

        let _dev2 = assert_some!(aux_register_device(DEV2, true));
        assert!(aux_free_balance(DEV1) > PENALTY);

        let _dev3 = assert_some!(aux_register_device(DEV3, true));
        assert!(aux_free_balance(DEV3) < PENALTY);

        assert_ok!(aux_order(DEV1, CL1, DEFAULT_FEE, DEFAULT_WCD));

        assert_eq!(aux_free_balance(CL3), 0);
        assert!(aux_free_balance(CL2) > DEFAULT_FEE);
        // client balance
        assert_err!(
            aux_order(DEV2, CL3, DEFAULT_FEE, DEFAULT_WCD),
            Error::<Test>::DeviceLowBail
        );
        //amend
        assert_ok!(aux_order(DEV2, CL2, DEFAULT_FEE, DEFAULT_WCD));
        // device collateral
        assert_err!(
            aux_order(DEV3, CL2, DEFAULT_FEE, DEFAULT_WCD),
            pallet_balances::Error::<Test, _>::InsufficientBalance
        );

        // amend
        assert_ok!(Balances::force_transfer(
            RawOrigin::Root.into(),
            account(1),
            account(DEV3),
            PENALTY
        ));
        assert_ok!(aux_order(DEV3, CL2, DEFAULT_FEE, DEFAULT_WCD));
    });
}

#[test]
fn order_reject() {
    new_test_ext().execute_with(|| {
        let devid = account(DEV1);
        let _dev1 = assert_some!(aux_register_device(DEV1, true));

        let b1 = aux_free_balance(CL1);
        assert_ok!(aux_order(DEV1, CL1, DEFAULT_FEE, DEFAULT_WCD));

        let b2 = aux_free_balance(CL1);
        assert_eq!(b1 - b2, DEFAULT_FEE);
        let dev1 = assert_some!(ServiceModule::devices(&devid));
        assert_eq!(dev1.state, DeviceState::Busy);

        //reject
        assert_ok!(ServiceModule::accept(
            Origin::signed(devid.clone()),
            true,
            true
        ));
        let b3 = aux_free_balance(CL1);
        assert_eq!(b1, b3);

        let dev1 = assert_some!(ServiceModule::devices(&devid));
        assert_eq!(dev1.state, DeviceState::Ready);
    });
}

#[test]
fn full_circle() {
    new_test_ext().execute_with(|| {
        let devid = account(DEV1);
        let _dev1 = assert_some!(aux_register_device(DEV1, true));
        let td1 = aux_total_balance(DEV1);

        let b1 = aux_free_balance(CL1);
        assert_ok!(aux_order(DEV1, CL1, DEFAULT_FEE, DEFAULT_WCD * 3));

        let b2 = aux_free_balance(CL1);
        assert_eq!(b1 - b2, DEFAULT_FEE);
        let dev1 = assert_some!(ServiceModule::devices(&devid));
        assert_eq!(dev1.state, DeviceState::Busy);

        assert_ok!(ServiceModule::accept(
            Origin::signed(devid.clone()),
            false,
            true
        ));

        crate::Timestamp::<Test>::set_timestamp(DEFAULT_WCD * 2);
        let dev1 = assert_some!(ServiceModule::devices(&devid));
        assert_eq!(dev1.state, DeviceState::Busy2);

        let tb1 = aux_total_balance(CL1);
        assert_eq!(b2, tb1 - DEFAULT_FEE);

        let order = ServiceModule::orders(&devid);
        assert!(order.is_some());

        assert_ok!(ServiceModule::done(Origin::signed(devid.clone()), true));
        let dev1 = assert_some!(ServiceModule::devices(&devid));
        assert_eq!(dev1.state, DeviceState::Ready);

        let tb2 = aux_total_balance(CL1);
        assert_eq!(b2, tb2);

        let order = ServiceModule::orders(&devid);
        assert!(order.is_none());

        let td2 = aux_total_balance(DEV1);
        assert_eq!(td2 - td1, DEFAULT_FEE);
    });
}

#[test]
fn order_cancel() {
    new_test_ext().execute_with(|| {
        let devid = account(DEV1);
        let b1 = aux_free_balance(CL1);

        let _order = aux_init_order(DEV1, CL1, DEFAULT_WCD * 10);
        crate::Timestamp::<Test>::set_timestamp(DEFAULT_WCD * 1);
        assert_ok!(ServiceModule::accept(
            Origin::signed(devid.clone()),
            false,
            true
        ));
        let order = ServiceModule::orders(&devid);
        assert!(order.is_some());

        let b2 = aux_free_balance(CL1);
        assert_eq!(b1 - b2, DEFAULT_FEE);

        crate::Timestamp::<Test>::set_timestamp(DEFAULT_WCD * 11);

        assert_ok!(ServiceModule::cancel(
            Origin::signed(account(CL1)),
            devid.clone()
        ));

        let b3 = aux_free_balance(CL1);
        let order = ServiceModule::orders(&devid);
        assert!(order.is_none());
        let dev = assert_some!(ServiceModule::devices(account(DEV1)));
        assert_eq!(dev.state, DeviceState::Busy2);

        let t3 = aux_total_balance(CL1);
        assert_eq!(b3 - b1, PENALTY);
        assert_eq!(b3, t3);
    });
}

#[test]
fn order_try_accept_foreign() {
    new_test_ext().execute_with(|| {
        let _order = aux_init_order(DEV1, CL1, DEFAULT_WCD * 10);
        crate::Timestamp::<Test>::set_timestamp(DEFAULT_WCD * 1);
        assert_err!(
            ServiceModule::accept(Origin::signed(account(DEV2)), false, true),
            Error::<Test>::NoDevice
        );
    });
}

#[test]
fn order_hasty_cancel() {
    new_test_ext().execute_with(|| {
        let devid = account(DEV1);
        let _order = aux_init_order(DEV1, CL1, DEFAULT_WCD * 10);
        crate::Timestamp::<Test>::set_timestamp(DEFAULT_WCD * 1);
        //assert_ok!(ServiceModule::accept(Origin::signed(account(DEV1)), false));
        let order = ServiceModule::orders(&devid);
        assert!(order.is_some());

        crate::Timestamp::<Test>::set_timestamp(DEFAULT_WCD * 2);
        assert_err!(
            ServiceModule::cancel(Origin::signed(account(CL1)), devid.clone()),
            Error::<Test>::Prohibited
        );

        // amend
        crate::Timestamp::<Test>::set_timestamp(DEFAULT_WCD * 10);
        assert_ok!(ServiceModule::cancel(
            Origin::signed(account(CL1)),
            devid.clone()
        ));
    });
}

#[test]
fn delay_accept() {
    new_test_ext().execute_with(|| {
        let devid = account(DEV1);
        let d1 = aux_total_balance(DEV1);
        let _order = aux_init_order(DEV1, CL1, DEFAULT_WCD * 10);

        crate::Timestamp::<Test>::set_timestamp(DEFAULT_WCD * 11);
        // try accept
        assert_err!(
            ServiceModule::accept(Origin::signed(devid.clone()), false, true),
            Error::<Test>::Overdue
        );
        let order = ServiceModule::orders(&devid);
        assert!(order.is_some());
        let dev = assert_some!(ServiceModule::devices(&devid));
        assert_eq!(dev.state, DeviceState::Busy);

        // amend
        // reject
        assert_ok!(ServiceModule::accept(
            Origin::signed(devid.clone()),
            true,
            false
        ));
        let order = ServiceModule::orders(&devid);
        assert!(order.is_none());
        let dev = assert_some!(ServiceModule::devices(&devid));
        assert_eq!(dev.state, DeviceState::Off);
        let d2 = aux_total_balance(DEV1);

        assert_eq!(d1 - d2, PENALTY);
    });
}

#[test]
fn delay_confirm() {
    new_test_ext().execute_with(|| {
        let d1 = aux_total_balance(DEV1);
        let _order = aux_init_order(DEV1, CL1, DEFAULT_WCD * 10);
        // accept
        assert_ok!(ServiceModule::accept(
            Origin::signed(account(DEV1)),
            false,
            true
        ));
        let d2 = aux_total_balance(DEV1);
        assert_eq!(d1, d2);
        // delay
        crate::Timestamp::<Test>::set_timestamp(DEFAULT_WCD * 11);
        assert_ok!(ServiceModule::done(Origin::signed(account(DEV1)), true));

        let d3 = aux_total_balance(DEV1);
        assert_eq!(d3 - d1, DEFAULT_FEE - PENALTY);
    });
}

#[test]
fn device_off_after_confirm() {
    new_test_ext().execute_with(|| {
        let devid = account(DEV1);
        let _order = aux_init_order(DEV1, CL1, DEFAULT_WCD * 10);
        assert_ok!(ServiceModule::accept(
            Origin::signed(devid.clone()),
            false,
            true
        ));

        crate::Timestamp::<Test>::set_timestamp(DEFAULT_WCD * 1);
        // turn off after confirm
        // assert_ok!(
        //     ServiceModule::set_state(Origin::signed( devid.clone() ), false )
        // );
        // let dev = assert_some!( ServiceModule::devices( &devid ) );
        // assert_eq!(dev.state, DeviceState::Standby);

        let order = ServiceModule::orders(&devid);
        assert!(order.is_some());

        // confirm
        assert_ok!(ServiceModule::done(Origin::signed(devid.clone()), false));

        let dev = assert_some!(ServiceModule::devices(&devid));
        assert_eq!(dev.state, DeviceState::Off);

        let order = ServiceModule::orders(&devid);
        assert!(order.is_none());
    });
}

#[test]
fn device_reregister() {
    new_test_ext().execute_with(|| {
        let devid = account(DEV1);
        let _order = aux_init_order(DEV1, CL1, DEFAULT_WCD * 10);
        assert_ok!(ServiceModule::accept(
            Origin::signed(devid.clone()),
            false,
            true
        ));

        assert_err!(
            ServiceModule::register(
                Origin::signed(devid.clone()),
                PENALTY * 2,
                DEFAULT_WCD * 2,
                true
            ),
            Error::<Test>::DeviceExists
        );
        // amend
        assert_ok!(ServiceModule::done(Origin::signed(devid.clone()), false));
        let dev = assert_some!(ServiceModule::devices(&devid));
        assert_eq!(dev.state, DeviceState::Off);

        assert_ok!(ServiceModule::register(
            Origin::signed(devid.clone()),
            PENALTY * 2,
            DEFAULT_WCD * 2,
            true
        ),);
        let dev = assert_some!(ServiceModule::devices(&devid));
        assert_eq!(dev.state, DeviceState::Ready);
    });
}

#[test]
fn order_abandoned() {
    new_test_ext().execute_with(|| {
        let d1 = aux_total_balance(DEV1);
        let devid = account(DEV1);
        let _order = aux_init_order(DEV1, CL1, DEFAULT_WCD * 10);

        crate::Timestamp::<Test>::set_timestamp(DEFAULT_WCD * 11);

        assert_ok!(ServiceModule::cancel(
            Origin::signed(account(CL1)),
            devid.clone()
        ));
        let d2 = aux_total_balance(DEV1);

        assert_eq!(d1 - d2, PENALTY);
        let dev = assert_some!(ServiceModule::devices(&devid));
        // abandoned device has busy state
        assert_eq!(dev.state, DeviceState::Busy);

        assert_err!(
            aux_order(DEV1, CL1, DEFAULT_FEE, DEFAULT_WCD * 10),
            Error::<Test>::IllegalState
        );
    });
}
