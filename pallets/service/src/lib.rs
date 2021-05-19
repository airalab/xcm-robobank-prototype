#![cfg_attr(not(feature = "std"), no_std)]
#![feature(or_patterns)]
#![allow(unused_imports)]

use codec::{Decode, Encode};
use frame_support::traits::OnKilledAccount;
use frame_support::{
    dispatch::DispatchResult,
    sp_runtime::traits::Hash,
    sp_runtime::RuntimeDebug,
    traits::{BalanceStatus::Free, Currency, Get, ReservableCurrency},
};

use cumulus_primitives_core::{
    relay_chain,
    well_known_keys::{self, NEW_VALIDATION_CODE},
    AbridgedHostConfiguration, DownwardMessageHandler, InboundDownwardMessage, InboundHrmpMessage,
    MessageSendError, OnValidationData, OutboundHrmpMessage, ParaId, PersistedValidationData,
    ServiceQuality, UpwardMessage, UpwardMessageSender, XcmpMessageHandler, XcmpMessageSender,
};

pub use pallet::*;
pub use pallet_common::DeviceState;
use pallet_common::*;
use sp_std::convert::{TryFrom, TryInto};
use sp_std::prelude::*;
use xcm::v0::{Error as XcmError, Junction, MultiLocation, OriginKind, SendXcm, Xcm};
use xcm::VersionedXcm;

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
}

pub trait OnReceived<T: Config> {
    fn on_received(
        device: &<T as frame_system::Config>::AccountId,
        order: &OrderOf<T>,
    ) -> Option<DeviceState>;
}

impl<T: Config> OnReceived<T> for () {
    fn on_received(
        _device: &<T as frame_system::Config>::AccountId,
        _order: &OrderOf<T>,
    ) -> Option<DeviceState> {
        Some(DeviceState::Busy)
    }
}

pub type OrderOf<T> = Order<
    <T as Config>::OrderPayload,
    BalanceOf<T>,
    MomentOf<T>,
    <T as frame_system::Config>::AccountId,
    ParaId,
>;

pub type OrderBaseOf<T> = OrderBase<
    <T as Config>::OrderPayload,
    BalanceOf<T>,
    MomentOf<T>,
    <T as frame_system::Config>::AccountId,
>;

pub type BalanceOf<T> =
    <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
pub type MomentOf<T> = <T as pallet_timestamp::Config>::Moment;
type Timestamp<T> = pallet_timestamp::Pallet<T>;

type XCMPMessageOf<T> = XCMPMessage<
    <T as frame_system::Config>::AccountId,
    BalanceOf<T>,
    <T as Config>::OrderPayload,
    <T as pallet_timestamp::Config>::Moment,
>;

#[frame_support::pallet]
pub mod pallet {
    #![allow(clippy::unused_unit)]

    use frame_support::dispatch::{Dispatchable, PostDispatchInfo};
    use frame_support::traits::{
        BalanceStatus, Currency, EnsureOrigin, Get, OnUnbalanced, ReservableCurrency,
    };
    use frame_support::{
        dispatch::{DispatchResult, DispatchResultWithPostInfo},
        pallet_prelude::*,
    };
    use frame_system::pallet_prelude::*;
    use sp_std::prelude::*;
    use xcm_executor::traits::ConvertOrigin;

    use super::{
        BalanceOf, DeviceProfile, DeviceState, Junction, MomentOf, OnReceived, OrderBaseOf,
        OrderOf, OriginKind, ParaId, SendXcm, Timestamp, Xcm, XcmpMessageSender,
    };

    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_timestamp::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
        type Currency: ReservableCurrency<Self::AccountId>;
        type OrderPayload: Encode + Decode + Clone + Default + Parameter;
        /// XCM interface
        type XcmpMessageSender: XcmpMessageSender;
        //type XcmSender: SendXcm;
        /// Own parachain Id
        type SelfParaId: Get<ParaId>;
        /// Call when new order received
        type OnReceived: OnReceived<Self>;
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);

    /// Device profiles
    #[pallet::storage]
    #[pallet::getter(fn devices)]
    pub type Device<T: Config> = StorageMap<
        _,
        Twox64Concat,
        <T as frame_system::Config>::AccountId,
        DeviceProfile<T>,
        OptionQuery,
    >;

    /// Device order store
    #[pallet::storage]
    #[pallet::getter(fn orders)]
    pub type Orders<T: Config> = StorageMap<
        _,
        Twox64Concat,
        <T as frame_system::Config>::AccountId,
        OrderOf<T>,
        OptionQuery,
    >;

    #[pallet::event]
    #[pallet::metadata(T::AccountId = "AccountId")]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        NewDevice(T::AccountId),
        NewOrder(T::AccountId),
        Accept(T::AccountId),
        Reject(T::AccountId),
        Done(T::AccountId),
        BadVersion(<T as frame_system::Config>::Hash),
        MessageReceived(Vec<u8>),
    }

    // Errors inform users that something went wrong.
    #[pallet::error]
    pub enum Error<T> {
        NoneValue,
        Prohibited,
        DeviceExists,
        DeviceLowBail,
        NoOrder,
        BadOrderDetails,
        NoDevice,
        IllegalState,
        Overdue,
        CannotReachDestination,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(10_000)]
        pub fn order(origin: OriginFor<T>, order: OrderBaseOf<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let OrderBaseOf::<T> {
                data,
                until,
                fee,
                device,
            } = order;
            let order = OrderOf::<T> {
                fee,
                data,
                until,
                paraid: T::SelfParaId::get(),
                client: who.into(),
            };

            Self::order_received(order, device)
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
            Self::order_reject(Some(&order), now, device, &mut dev, false)
        }

        #[pallet::weight(10_000)]
        pub fn accept(origin: OriginFor<T>, reject: bool, onoff: bool) -> DispatchResult {
            let id = ensure_signed(origin)?;

            let order = Orders::<T>::get(&id);

            let mut dev = Device::<T>::get(&id).ok_or(Error::<T>::NoDevice)?;

            let now = Timestamp::<T>::get();
            if reject {
                if !matches!(dev.state, DeviceState::Busy | DeviceState::Busy2) {
                    return Err(Error::<T>::IllegalState.into());
                }
                dev.state = if onoff {
                    DeviceState::Ready
                } else {
                    DeviceState::Off
                };
                return Self::order_reject(order.as_ref(), now, id, &mut dev, onoff);
            }
            if dev.state != DeviceState::Busy {
                return Err(Error::<T>::IllegalState.into());
            }
            let order = order.ok_or(Error::<T>::NoOrder)?;

            if now >= order.until {
                return Err(Error::<T>::Overdue.into());
            }

            Self::order_accept(&order, now, id, &mut dev);
            Ok(())
        }

        #[pallet::weight(10_000)]
        pub fn done(origin: OriginFor<T>, onoff: bool) -> DispatchResult {
            let id = ensure_signed(origin)?;

            let mut dev = Device::<T>::get(&id).ok_or(Error::<T>::NoDevice)?;

            if dev.state != DeviceState::Busy2 {
                return Err(Error::<T>::IllegalState.into());
            }

            let order = Orders::<T>::take(&id).ok_or(Error::<T>::NoOrder)?;
            let now = Timestamp::<T>::get();

            Self::order_done(&order, now, id, &mut dev, onoff)
        }

        #[pallet::weight(10_000)]
        pub fn register(
            origin: OriginFor<T>,
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
                },
            );
            Self::deposit_event(Event::NewDevice(id));
            Ok(())
        }

        #[pallet::weight(10_000)]
        pub fn set_state(origin: OriginFor<T>, onoff: bool) -> DispatchResult {
            let id = ensure_signed(origin)?;

            Device::<T>::try_mutate(&id, |d| {
                if let Some(ref mut dev) = d {
                    dev.state = match (onoff, &dev.state) {
                        (false, DeviceState::Ready | DeviceState::Off) => DeviceState::Off,
                        //(false, DeviceState::Busy2 | DeviceState::Standby) => DeviceState::Standby,
                        (true, DeviceState::Off) => DeviceState::Ready,
                        (_) => return Err(Error::<T>::IllegalState.into()),
                    };
                    Ok(())
                } else {
                    Err(Error::<T>::NoDevice.into())
                }
            })
        }
    }
}

impl<T: Config> Pallet<T> {
    pub fn order_received(order: OrderOf<T>, device: T::AccountId) -> DispatchResult {
        let now = Timestamp::<T>::get();

        if now >= order.until {
            return Err(Error::<T>::Overdue.into());
        }

        if Orders::<T>::contains_key(&device) {
            return Err(Error::<T>::IllegalState.into());
        }

        let mut dev = Device::<T>::get(&device).ok_or(Error::<T>::NoDevice)?;
        if dev.state != DeviceState::Ready {
            return Err(Error::<T>::IllegalState.into());
        }

        if order.until < (now + dev.wcd) {
            return Err(Error::<T>::BadOrderDetails.into());
        }

        dev.state = T::OnReceived::on_received(&device, &order).ok_or(Error::<T>::IllegalState)?;

        debug_assert!(matches!(dev.state, DeviceState::Busy | DeviceState::Busy2));

        if order.paraid == T::SelfParaId::get() {
            if !T::Currency::can_reserve(&order.client, order.fee) {
                return Err(Error::<T>::DeviceLowBail.into());
            }
            T::Currency::reserve(&device, dev.penalty)?;
            T::Currency::reserve(&order.client, order.fee)?;
        }

        Orders::<T>::insert(&device, &order);
        Self::deposit_event(Event::NewOrder(device.clone()));

        if dev.state == DeviceState::Busy2 {
            Self::order_accept(&order, now, device, &mut dev);
        } else {
            Device::<T>::insert(&device, &dev);
        }
        Ok(())
    }

    fn order_done(
        order: &OrderOf<T>,
        now: T::Moment,
        device: T::AccountId,
        dev: &mut DeviceProfile<T>,
        onoff: bool,
    ) -> DispatchResult {
        // TODO Send XCM with done
        dev.state = if onoff {
            DeviceState::Ready
        } else {
            DeviceState::Off
        };

        let para_id = T::SelfParaId::get();
        Device::<T>::insert(&device, &*dev);

        if order.paraid == para_id {
            T::Currency::repatriate_reserved(&order.client, &device, order.fee, Free)?;

            if now < order.until {
                T::Currency::unreserve(&device, dev.penalty);
            } else {
                T::Currency::repatriate_reserved(&device, &order.client, dev.penalty, Free)?;
            }
        } else {
            log::info!("send OrderDone message");
            let msg: XCMPMessageOf<T> =
                XCMPMessageOf::<T>::OrderDone(order.client.clone(), device.clone(), onoff);
            T::XcmpMessageSender::send_blob_message(
                order.paraid,
                msg.encode(),
                ServiceQuality::Ordered,
            )
            .map_err(|_| Error::<T>::CannotReachDestination)?;
            log::info!("OrderDone's sent");
        }

        Self::deposit_event(Event::Done(device));
        Ok(())
    }

    fn order_accept(
        order: &OrderOf<T>,
        _now: T::Moment,
        device: T::AccountId,
        dev: &mut DeviceProfile<T>,
    ) {
        dev.state = DeviceState::Busy2;
        Device::<T>::insert(&device, &*dev);
        let para_id = T::SelfParaId::get();

        if order.paraid != para_id {
            let msg: XCMPMessageOf<T> =
                XCMPMessageOf::<T>::OrderAccept(order.client.clone(), device.clone());
            T::XcmpMessageSender::send_blob_message(
                order.paraid,
                msg.encode(),
                ServiceQuality::Ordered,
            )
            .map_err(|_| Error::<T>::CannotReachDestination);
        }

        Self::deposit_event(Event::Accept(device));
    }

    fn order_reject(
        order: Option<&OrderOf<T>>,
        now: T::Moment,
        device: T::AccountId,
        dev: &mut DeviceProfile<T>,
        onoff: bool,
    ) -> DispatchResult {
        if let Some(order) = order {
            let para_id = T::SelfParaId::get();

            if order.paraid == para_id {
                T::Currency::unreserve(&order.client, order.fee);
                if now < order.until {
                    T::Currency::unreserve(&device, dev.penalty);
                } else {
                    T::Currency::repatriate_reserved(&device, &order.client, dev.penalty, Free)?;
                }
            } else {
                log::info!("send OrderReject message");
                let msg: XCMPMessageOf<T> =
                    XCMPMessageOf::<T>::OrderReject(order.client.clone(), device.clone(), onoff);
                T::XcmpMessageSender::send_blob_message(
                    order.paraid,
                    msg.encode(),
                    ServiceQuality::Ordered,
                )
                .map_err(|_| Error::<T>::CannotReachDestination)?;
                log::info!("OrderReject's sent");
            }
        }
        Orders::<T>::remove(&device);

        Device::<T>::insert(&device, &*dev);
        Self::deposit_event(Event::Reject(device));

        Ok(())
    }
}

impl<T: Config> OnKilledAccount<T::AccountId> for Pallet<T> {
    /// The account with the given id was reaped.
    fn on_killed_account(who: &T::AccountId) {
        //Timewait
        if let Some(dev) = Device::<T>::get(who) {
            if dev.state == DeviceState::Off {
                Device::<T>::remove(who);
            }
        }
    }
}

impl<T: Config> XcmpMessageHandler for Pallet<T> {
    fn handle_xcm_message(_sender: ParaId, _sent_at: relay_chain::BlockNumber, xcm: VersionedXcm) {
        let hash = xcm.using_encoded(T::Hashing::hash);
        log::info!("Processing HRMP XCM: {:?}", &hash);
        match Xcm::try_from(xcm) {
            Ok(_xcm) => {
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
    fn handle_blob_message(sender: ParaId, _sent_at: relay_chain::BlockNumber, blob: Vec<u8>) {
        log::warn!("Processing Blob XCM: {:?}", &blob);
        match XCMPMessageOf::<T>::decode(&mut blob.as_slice()) {
            Err(e) => {
                log::error!("{:?}", e);
                return;
            }
            Ok(XCMPMessageOf::<T>::NewOrder(client, order)) => {
                let OrderBaseOf::<T> {
                    data,
                    until,
                    fee,
                    device,
                } = order;
                let order = OrderOf::<T> {
                    fee,
                    data,
                    until,
                    paraid: sender,
                    client,
                };
                log::info!("new order received for {:?}", &device);

                match Self::order_received(order, device) {
                    Err(e) => {
                        log::error!("order_received return {:?}", e);
                    }
                    Ok(_) => {
                        log::info!("order_received succeed");
                    }
                }
            }
            Ok(_) => {
                log::warn!("unknown XCMP message received");
                return;
            }
        };
        //debug_assert!(false, "Blob messages not handled.")
    }
}
