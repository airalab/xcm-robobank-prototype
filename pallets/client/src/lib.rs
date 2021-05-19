#![cfg_attr(not(feature = "std"), no_std)]
#![feature(or_patterns)]
#![allow(unused_imports)]

use codec::{Decode, Encode};
use frame_support::{
    dispatch::DispatchResult,
    sp_runtime::traits::Hash,
    sp_runtime::RuntimeDebug,
    traits::{BalanceStatus::Free, Currency, Get, ReservableCurrency},
};
pub use pallet::*;
use sp_std::prelude::*;
use xcm::v0::{Junction, OriginKind, SendXcm, Xcm};

use sp_std::convert::{TryFrom, TryInto};

use cumulus_primitives_core::{
    relay_chain,
    well_known_keys::{self, NEW_VALIDATION_CODE},
    AbridgedHostConfiguration, DownwardMessageHandler, InboundDownwardMessage, InboundHrmpMessage,
    MessageSendError, OnValidationData, OutboundHrmpMessage, ParaId, PersistedValidationData,
    ServiceQuality, UpwardMessage, UpwardMessageSender, XcmpMessageHandler, XcmpMessageSender,
};
use frame_support::traits::OnKilledAccount;
use pallet_common::*;
use xcm::VersionedXcm;

type XCMPMessageOf<T> = XCMPMessage<
    <T as frame_system::Config>::AccountId,
    BalanceOf<T>,
    <T as Config>::OrderPayload,
    <T as pallet_timestamp::Config>::Moment,
>;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg_attr(feature = "std", derive(PartialEq, Debug))]
#[derive(Encode, Decode, Default)]
pub struct DeviceProfile<T: Config> {
    /// Device state
    state: DeviceState,
    /// Device collateral value
    penalty: BalanceOf<T>,
    /// Work circle duration
    wcd: MomentOf<T>,
    /// Parachain Id
    paraid: ParaId,
}
pub(crate) type OrderBaseOf<T> = OrderBase<
    <T as Config>::OrderPayload,
    BalanceOf<T>,
    MomentOf<T>,
    <T as frame_system::Config>::AccountId,
>;

pub(crate) type OrderOf<T> = Order<
    <T as Config>::OrderPayload,
    BalanceOf<T>,
    MomentOf<T>,
    <T as frame_system::Config>::AccountId,
    ParaId,
>;

pub type BalanceOf<T> =
    <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
pub type MomentOf<T> = <T as pallet_timestamp::Config>::Moment;
type Timestamp<T> = pallet_timestamp::Pallet<T>;

#[frame_support::pallet]
pub mod pallet {
    #![allow(clippy::unused_unit)]
    use super::{
        BalanceOf, DeviceProfile, DeviceState, Junction, MomentOf, OrderBaseOf, OrderOf,
        OriginKind, ParaId, ReservableCurrency, SendXcm, ServiceQuality, Timestamp, Xcm,
        XcmpMessageSender,
    };
    use crate::XCMPMessageOf;
    use frame_support::dispatch::{Dispatchable, PostDispatchInfo};
    use frame_support::{dispatch::DispatchResultWithPostInfo, pallet_prelude::*};
    use frame_system::pallet_prelude::*;
    use sp_std::prelude::*;
    use xcm_executor::traits::ConvertOrigin;

    /// Configure the pallet by specifying the parameters and types on which it depends.
    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_timestamp::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
        //type XcmSender: SendXcm;
        /// Use cumulus Xcm API
        type XcmpMessageSender: XcmpMessageSender;
        type Currency: ReservableCurrency<Self::AccountId>;
        type OrderPayload: Encode + Decode + Clone + Default + Parameter;
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);

    /// Device profiles
    #[pallet::storage]
    #[pallet::getter(fn devices)]
    pub type Device<T: Config> =
        StorageMap<_, Twox64Concat, T::AccountId, DeviceProfile<T>, OptionQuery>;

    /// Device order store
    #[pallet::storage]
    #[pallet::getter(fn orders)]
    pub type Orders<T: Config> = StorageMap<_, Twox64Concat, T::AccountId, OrderOf<T>, OptionQuery>;

    #[pallet::event]
    #[pallet::metadata(T::AccountId = "AccountId")]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        NewDevice(T::AccountId),
        NewOrder(T::AccountId, T::AccountId),
        Accept(T::AccountId, T::AccountId),
        Reject(T::AccountId, T::AccountId),
        Done(T::AccountId, T::AccountId),
        BadVersion(<T as frame_system::Config>::Hash),
    }

    // Errors inform users that something went wrong.
    #[pallet::error]
    pub enum Error<T> {
        /// Error names should be descriptive.
        NoneValue,
        OrderExists,
        IllegalState,
        Overdue,
        DeviceLowBail,
        DeviceExists,
        BadOrderDetails,
        NoDevice,
        NoOrder,
        Prohibited,
        CannotReachDestination,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(10_000)]
        pub fn test(origin: OriginFor<T>) -> DispatchResult {
            T::XcmpMessageSender::send_blob_message(
                (200).into(),
                vec![0x00, 0x20],
                ServiceQuality::Ordered,
            )
            .map_err(|_| Error::<T>::CannotReachDestination.into())
            .map(|_| ())
        }

        #[pallet::weight(10_000)]
        pub fn order(origin: OriginFor<T>, order: OrderBaseOf<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let now = Timestamp::<T>::get();

            if now >= order.until {
                return Err(Error::<T>::Overdue.into());
            }

            if Orders::<T>::contains_key(&order.device) {
                return Err(Error::<T>::IllegalState.into());
            };

            let mut dev = Device::<T>::get(&order.device).ok_or(Error::<T>::NoDevice)?;
            if dev.state != DeviceState::Ready {
                return Err(Error::<T>::IllegalState.into());
            }

            if order.until < (now + dev.wcd) {
                return Err(Error::<T>::BadOrderDetails.into());
            };

            if !T::Currency::can_reserve(&who, order.fee) {
                return Err(Error::<T>::DeviceLowBail.into());
            }

            T::Currency::reserve(&order.device, dev.penalty)?;
            T::Currency::reserve(&who, order.fee)?;

            let device = order.device.clone();
            // store order
            let order: OrderBaseOf<T> = {
                let order: OrderOf<T> = order.convert(who.clone());
                Orders::<T>::insert(&device, &order);
                order.convert(device.clone())
            };

            let msg: XCMPMessageOf<T> = XCMPMessageOf::<T>::NewOrder(who.clone(), order);

            log::info!("send XCM order message");

            T::XcmpMessageSender::send_blob_message(
                dev.paraid,
                msg.encode(),
                ServiceQuality::Ordered,
            )
            .map_err(|_| Error::<T>::CannotReachDestination)?;
            log::info!("XCM order message has sent");
            dev.state = DeviceState::Busy;
            Device::<T>::insert(&device, &dev);

            Self::deposit_event(Event::NewOrder(who, device.clone()));

            Ok(())
        }

        #[pallet::weight(10_000)]
        pub fn cancel(origin: OriginFor<T>, device: T::AccountId) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let order = Orders::<T>::get(&device).ok_or(Error::<T>::NoOrder)?;

            let now = Timestamp::<T>::get();

            if now < order.until || order.client != who {
                return Err(Error::<T>::Prohibited.into());
            }

            let mut dev = Device::<T>::get(&device).ok_or(Error::<T>::NoDevice)?;
            // Note. we don't change device state
            Self::order_reject(who, &order, now, device, &mut dev)
        }

        #[pallet::weight(10_000)]
        pub fn register(
            origin: OriginFor<T>,
            paraid: ParaId,
            penalty: BalanceOf<T>,
            wcd: MomentOf<T>,
            onoff: bool,
        ) -> DispatchResult {
            let id = ensure_signed(origin)?;

            if Orders::<T>::contains_key(&id) {
                return Err(Error::<T>::DeviceExists.into());
            }
            // Despite the order doesn't exist, device can be in Busy,Busy2 state.
            //
            Device::<T>::insert(
                &id,
                DeviceProfile {
                    wcd,
                    penalty,
                    state: if onoff {
                        DeviceState::Ready
                    } else {
                        DeviceState::Off
                    },
                    paraid,
                },
            );

            Self::deposit_event(Event::NewDevice(id));
            Ok(())
        }
    }
}

impl<T: Config> Pallet<T> {
    fn on_accept(who: T::AccountId, device: T::AccountId) -> DispatchResult {
        //let order = Orders::<T>::get(&device).ok_or(Error::<T>::NoOrder)?;
        Self::deposit_event(Event::Accept(who, device));
        Ok(())
    }

    fn on_reject(who: T::AccountId, device: T::AccountId, onoff: bool) -> DispatchResult {
        let order = Orders::<T>::get(&device).ok_or(Error::<T>::NoOrder)?;

        //TODO order.client== who
        let now = Timestamp::<T>::get();
        let mut dev = Device::<T>::get(&device).ok_or(Error::<T>::NoDevice)?;

        dev.state = if !onoff {
            DeviceState::Off
        } else {
            DeviceState::Ready
        };

        Self::order_reject(who, &order, now, device, &mut dev)
    }

    fn on_done(who: T::AccountId, device: T::AccountId, onoff: bool) -> DispatchResult {
        let order = Orders::<T>::get(&device).ok_or(Error::<T>::NoOrder)?;
        //TODO order.client== who
        let now = Timestamp::<T>::get();
        let mut dev = Device::<T>::get(&device).ok_or(Error::<T>::NoDevice)?;

        T::Currency::repatriate_reserved(&who, &device, order.fee, Free)?;

        if now < order.until {
            T::Currency::unreserve(&device, dev.penalty);
        } else {
            T::Currency::repatriate_reserved(&device, &who, dev.penalty, Free)?;
        }
        Orders::<T>::remove(&device);

        dev.state = if !onoff {
            DeviceState::Off
        } else {
            DeviceState::Ready
        };

        Device::<T>::insert(&device, &dev);
        Self::deposit_event(Event::Done(who, device));
        Ok(())
    }

    fn order_reject(
        who: T::AccountId,
        order: &OrderOf<T>,
        now: T::Moment,
        device: T::AccountId,
        dev: &mut DeviceProfile<T>,
    ) -> DispatchResult {
        T::Currency::unreserve(&who, order.fee);

        if now < order.until {
            T::Currency::unreserve(&device, dev.penalty);
        } else {
            T::Currency::repatriate_reserved(&device, &order.client, dev.penalty, Free)?;
        }

        Orders::<T>::remove(&device);
        Device::<T>::insert(&device, &*dev);

        Self::deposit_event(Event::Reject(who, device));
        Ok(())
    }
}

impl<T: Config> OnKilledAccount<T::AccountId> for Pallet<T> {
    /// The account with the given id was reaped.
    fn on_killed_account(who: &T::AccountId) {
        //Timewait
        if let Some(mut dev) = Device::<T>::get(who) {
            if dev.state == DeviceState::Off {
                Device::<T>::remove(who);
            } else {
                dev.state = DeviceState::Timewait;
                Device::<T>::insert(who, dev);
            }
        }
    }
}

impl<T: Config> XcmpMessageHandler for Pallet<T> {
    fn handle_xcm_message(sender: ParaId, _sent_at: relay_chain::BlockNumber, xcm: VersionedXcm) {
        let hash = xcm.using_encoded(T::Hashing::hash);
        log::debug!("Processing HRMP XCM: {:?}", &hash);
        match Xcm::try_from(xcm) {
            Ok(xcm) => {
                // let location = (
                //     Junction::Parent,
                //     Junction::Parachain { id: sender.into() },
                // );
                // match T::XcmExecutor::execute_xcm(location.into(), xcm) {
                //     Ok(..) => RawEvent::Success(hash),
                //     Err(e) => RawEvent::Fail(hash, e),
                // }
            }
            Err(..) => Self::deposit_event(Event::BadVersion(hash)),
        };
    }
    fn handle_blob_message(_sender: ParaId, _sent_at: relay_chain::BlockNumber, blob: Vec<u8>) {
        log::warn!("Processing Blob XCM: {:?}", blob);
        match XCMPMessageOf::<T>::decode(&mut blob.as_slice()) {
            Err(e) => {
                log::error!("{:?}", e);
                return;
            }
            Ok(XCMPMessageOf::<T>::OrderAccept(client, devid)) => {
                Self::on_accept(client, devid);
                log::info!("OrderAccept");
            }
            Ok(XCMPMessageOf::<T>::OrderReject(client, devid, onoff)) => {
                Self::on_reject(client, devid, onoff);
                log::info!("OrderReject");
            }
            Ok(XCMPMessageOf::<T>::OrderDone(cliend, devid, onoff)) => {
                Self::on_done(cliend, devid, onoff);
                log::info!("OrderDone");
            }
            Ok(_) => {
                log::warn!("unknown XCM message received");
            }
        };
    }
}
