#![allow(clippy::from_over_into)]

use crate as pallet_client;
use frame_support::parameter_types;
use frame_system as system;
use frame_system::EnsureRoot;
use sp_core::H256;
use sp_runtime::{
    testing::Header,
    traits::{BlakeTwo256, IdentifyAccount, IdentityLookup, Verify},
    MultiSignature,
};
use xcm_builder::{
    AccountId32Aliases, CurrencyAdapter, LocationInverter, ParentIsDefault, RelayChainAsNative,
    SiblingParachainAsNative, SiblingParachainConvertsVia, SignedAccountId32AsNative,
    SovereignSignedViaLocation,
};

use xcm::v0::{Junction, MultiLocation, NetworkId};
use xcm_executor::traits::{IsConcrete, NativeAsset};
use xcm_executor::{Config, XcmExecutor};

use polkadot_parachain::primitives::Id as ParaId;
use polkadot_parachain::primitives::Sibling;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
pub type Signature = MultiSignature;
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;
/// Balance of an account.
pub type Balance = u128;

parameter_types! {
    pub const RococoLocation: MultiLocation = MultiLocation::X1(Junction::Parent);
    pub const RococoNetwork: NetworkId = NetworkId::Polkadot;
    pub RelayChainOrigin: Origin = cumulus_pallet_xcm_handler::Origin::Relay.into();
    pub Ancestry: MultiLocation = Junction::Parachain {
        id: 999
    }.into();

    pub SelfParaId: ParaId = ParaId::from(999);
}

type LocationConverter = (
    ParentIsDefault<AccountId>,
    SiblingParachainConvertsVia<Sibling, AccountId>,
    AccountId32Aliases<RococoNetwork, AccountId>,
);
type LocalOriginConverter = (
    SovereignSignedViaLocation<LocationConverter, Origin>,
    RelayChainAsNative<RelayChainOrigin, Origin>,
    SiblingParachainAsNative<cumulus_pallet_xcm_handler::Origin, Origin>,
    SignedAccountId32AsNative<RococoNetwork, Origin>,
);

type LocalAssetTransactor = CurrencyAdapter<
    // Use this currency:
    Balances,
    // Use this currency when it is a fungible asset matching the given location or name:
    IsConcrete<RococoLocation>,
    // Do a simple punn to convert an AccountId32 MultiLocation into a native chain account ID:
    LocationConverter,
    // Our chain's account ID type (we can't get away without mentioning it explicitly):
    AccountId,
>;

pub struct XcmConfig;
impl Config for XcmConfig {
    type Call = Call;
    type XcmSender = XcmHandler;
    // How to withdraw and deposit an asset.
    type AssetTransactor = LocalAssetTransactor;
    type OriginConverter = LocalOriginConverter;
    type IsReserve = NativeAsset;
    type IsTeleporter = ();
    type LocationInverter = LocationInverter<Ancestry>;
}

impl cumulus_pallet_xcm_handler::Config for Test {
    type Event = Event;
    type XcmExecutor = XcmExecutor<XcmConfig>;
    type UpwardMessageSender = ParachainSystem;
    type HrmpMessageSender = ParachainSystem;
    type SendXcmOrigin = EnsureRoot<AccountId>;
    type AccountIdConverter = LocationConverter;
}

impl cumulus_pallet_parachain_system::Config for Test {
    type Event = Event;
    type OnValidationData = ();
    type SelfParaId = SelfParaId;
    type DownwardMessageHandlers = ();
    type HrmpMessageHandlers = XcmHandler;
}

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
    pub enum Test where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        System: frame_system::{Module, Call, Config, Storage, Event<T>},
        Balances: pallet_balances::{Module, Call, Storage, Config<T>, Event<T>},
        XcmHandler: cumulus_pallet_xcm_handler::{Event<T>, Origin},
        ParachainSystem: cumulus_pallet_parachain_system::{Module, Call, Storage, Inherent, Event},
        //XcmHandler: cumulus_pallet_xcm_handler::{Module, Event<T>, Origin},
        TemplateModule: pallet_template::{Module, Call, Storage, Event<T>},
    }
);

parameter_types! {
    pub const ExistentialDeposit: u128 = 500;
    pub const MaxLocks: u32 = 50;
}

impl pallet_balances::Config for Test {
    type MaxLocks = MaxLocks;
    /// The type for recording an account's balance.
    type Balance = Balance;
    /// The ubiquitous event type.
    type Event = Event;
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = ();
}

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const SS58Prefix: u8 = 42;
}

impl system::Config for Test {
    type BaseCallFilter = ();
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type Origin = Origin;
    type Call = Call;
    type Index = u64;
    type BlockNumber = u64;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Header = Header;
    type Event = Event;
    type BlockHashCount = BlockHashCount;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = pallet_balances::AccountData<Balance>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = SS58Prefix;
}

impl pallet_client::Config for Test {
    type Event = Event;
    type XcmSender = ();
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    system::GenesisConfig::default()
        .build_storage::<Test>()
        .unwrap()
        .into()
}
