#![cfg_attr(not(feature = "std"), no_std)]
use codec::{Decode, Encode};

use frame_support::{
    sp_runtime::traits::Hash,
    sp_runtime::RuntimeDebug,
    traits::{BalanceStatus::Free, Currency, Get, ReservableCurrency},
};

// use cumulus_primitives_core::{
//     relay_chain,
//     well_known_keys::{self, NEW_VALIDATION_CODE},
//     AbridgedHostConfiguration, DownwardMessageHandler, InboundDownwardMessage, InboundHrmpMessage,
//     MessageSendError, OnValidationData, OutboundHrmpMessage, ParaId, PersistedValidationData,
//     ServiceQuality, UpwardMessage, UpwardMessageSender, XcmpMessageHandler, XcmpMessageSender,
// };

#[cfg_attr(feature = "std", derive(Debug))]
#[derive(Encode, Decode, PartialEq)]
pub enum DeviceState {
    /// Device is off
    Off,
    /// Device is ready to accept orders
    Ready,
    /// Device has order
    Busy,
    /// Device has accepted order
    Busy2,
    /// Device is abandoned
    Timewait,
}
impl Default for DeviceState {
    fn default() -> Self {
        DeviceState::Off
    }
}

#[derive(Encode, Decode, Default, Clone, RuntimeDebug, PartialEq)]
pub struct OrderBase<Payload: Encode + Decode, Balance, Moment, AccountId> {
    pub until: Moment,
    pub data: Payload,
    pub fee: Balance,
    pub device: AccountId,
}

impl<Payload: Encode + Decode, Balance, Moment, AccountId>
    OrderBase<Payload, Balance, Moment, AccountId>
{
    pub fn convert<ParaId: From<u32>>(
        self,
        client: AccountId,
    ) -> Order<Payload, Balance, Moment, AccountId, ParaId> {
        Order {
            until: self.until,
            data: self.data,
            fee: self.fee,
            client,
            paraid: 0.into(),
        }
    }
}

//#[cfg_attr(feature = "std", derive(PartialEq))]
#[derive(Encode, Decode, Default, Clone, RuntimeDebug, PartialEq)]
pub struct Order<Payload: Encode + Decode, Balance, Moment, AccountId, ParaId> {
    pub until: Moment,
    pub data: Payload,
    pub fee: Balance,
    pub client: AccountId,
    pub paraid: ParaId,
}

impl<Payload: Encode + Decode, Balance, Moment, AccountId, ParaId>
    Order<Payload, Balance, Moment, AccountId, ParaId>
{
    pub fn convert(self, device: AccountId) -> OrderBase<Payload, Balance, Moment, AccountId> {
        OrderBase {
            until: self.until,
            data: self.data,
            fee: self.fee,
            device,
        }
    }
}

#[derive(codec::Encode, codec::Decode)]
pub enum XCMPMessage<XAccountId, XBalance, Payout: Encode + Decode, Moment> {
    /// Transfer tokens to the given account from the Parachain account.
    //TransferToken(XAccountId, XBalance),
    /// Order sent to device (client, order)
    NewOrder(XAccountId, OrderBase<Payout, XBalance, Moment, XAccountId>),
    /// Order accepted by device (clientid, deviceid)
    OrderAccept(XAccountId, XAccountId),
    /// Order rejected by device (clientid, deviceid, on/off)
    OrderReject(XAccountId, XAccountId, bool),
    /// Order completed (clientid, deviceid, on/off)
    OrderDone(XAccountId, XAccountId, bool),
}
