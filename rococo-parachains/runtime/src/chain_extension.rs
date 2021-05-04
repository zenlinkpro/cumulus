use pallet_contracts::chain_extension::{
    ChainExtension, Environment, Ext, InitState, RetVal, SysConfig, UncheckedFrom,
};
use sp_runtime::{DispatchError, AccountId32};
use super::Runtime;
use sp_std::mem;
use codec::{Encode, Decode};
use xcm_executor::XcmExecutor;
use crate::XcmConfig;
use xcm::v0::{MultiLocation, Junction, NetworkId, MultiAsset, Xcm, Order, ExecuteXcm};
use sp_std::vec;

pub struct XcmSenderExtension;

#[derive(Debug, Encode, Decode)]
struct XcmParameter {
    pub from:   AccountId32,
    pub target_chain_id: u32,
    pub amount :u128,
}

impl ChainExtension<Runtime> for XcmSenderExtension {
    fn call<E: Ext>(func_id: u32, env: Environment<E, InitState>) -> Result<RetVal, DispatchError>
        where
            <E::T as SysConfig>::AccountId: UncheckedFrom<<E::T as SysConfig>::Hash> + AsRef<[u8]>,
    {
        match func_id {
            1101 => {
                log::info!("chain extension step 0");

                let mut env = env.buf_in_buf_out();
                let mut data = env.read(mem::size_of::<XcmParameter>() as u32).unwrap();
                let params = XcmParameter::decode(&mut &data[..]).unwrap();
                log::info!("chain extension step 1 {:#?}", params);

                let origin = MultiLocation::from(Junction::AccountId32{ network: NetworkId::Any, id: <[u8; 32]>::from(params.from.clone()) });
                let xcm_msg = make_xcm_lateral_transfer_native(origin.clone(), params.target_chain_id, params.from, params.amount);

                let xcm_msg_v2 = Xcm::<>::from(xcm_msg);

                let res = XcmExecutor::<XcmConfig>::execute_xcm(origin, xcm_msg_v2, 1_000_000_000);
                log::info!("chain extension step 2 {:#?}", res);
            }

            _ => {
                log::info!("unknown chain extension");
            }
        };
        Ok(RetVal::Converging(0))
    }

    fn enabled() -> bool {
        true
    }
}

pub fn make_xcm_lateral_transfer_native(
    location: MultiLocation,
    para_id: u32,
    account: AccountId32,
    amount: u128,
) -> Xcm<()> {
    Xcm::WithdrawAsset {
        assets: vec![MultiAsset::ConcreteFungible { id: location, amount }],
        effects: vec![Order::DepositReserveAsset {
            assets: vec![MultiAsset::All],
            dest: MultiLocation::X2(
                Junction::Parent,
                Junction::Parachain { id: para_id.into() },
            ),
            effects: vec![make_deposit_asset_order(account)],
        }],
    }
}

fn make_deposit_asset_order(account: AccountId32) ->Order<()> {
    Order::DepositAsset {
        assets: vec![MultiAsset::All],
        dest: MultiLocation::X1(Junction::AccountId32 {
            network: NetworkId::Any,
            id: <[u8; 32]>::from(account),
        }),
    }
}
