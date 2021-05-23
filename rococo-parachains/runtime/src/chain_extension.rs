use codec::{Encode, Decode};

use sp_std::{mem, vec, prelude::*, marker::PhantomData};
use sp_runtime::{DispatchError, AccountId32, traits::Zero};
use xcm_executor::XcmExecutor;
use xcm::v0::{MultiLocation, Junction, NetworkId, MultiAsset, Xcm, Order, ExecuteXcm};
use pallet_contracts::chain_extension::{
    ChainExtension, Environment, Ext, InitState, RetVal, SysConfig, UncheckedFrom,
};

use rococo_parachain_primitives::AccountId;
use crate::{XcmConfig, Runtime, Balances, Tokens, contract_assets_adapter::{
    CustomMultiAsset, AssetId, CustomMultiAssetAdapter},
};
use sp_std::borrow::Borrow;
use frame_support::sp_runtime::traits::AccountIdConversion;
use sp_std::convert::TryFrom;

pub struct CustomExtension<CustomMultiAssetAdapter>(
    PhantomData<CustomMultiAssetAdapter>
);

#[derive(Debug, Encode, Decode)]
struct XcmParameter {
    pub from: AccountId32,
    pub target_chain_id: u32,
    pub amount: u128,
}

// Now, we ignore AssetId, just transfer Balance;
#[derive(Debug, Encode, Decode)]
struct TransferParameter {
    pub asset_id: AssetId,
    pub to: AccountId32,
    pub amount: u128,
}
#[derive(Debug, Encode, Decode)]
struct BalancesOfParameter{
    pub asset_id: AssetId,
    pub owner: AccountId32,
}

pub fn to_account_id(account: &[u8]) -> AccountId32 {
    AccountId32::try_from(account).unwrap()
}

impl ChainExtension<Runtime> for CustomExtension<CustomMultiAsset<AccountId, AssetId, Balances, Tokens>> {
    fn call<E: Ext>(func_id: u32, env: Environment<E, InitState>) -> Result<RetVal, DispatchError>
        where
            <E::T as SysConfig>::AccountId: UncheckedFrom<<E::T as SysConfig>::Hash> + AsRef<[u8]> ,
    {
        match func_id {
            1101 => {
                log::info!("chain extension step 0");
                let env = env.buf_in_buf_out();
                let data = env.read(mem::size_of::<XcmParameter>() as u32).unwrap();
                let params = XcmParameter::decode(&mut &data[..]).unwrap();
                log::info!("chain extension step 1 {:#?}", params);

                let origin = MultiLocation::from(
                    Junction::AccountId32 {
                        network: NetworkId::Any,
                        id: <[u8; 32]>::from(params.from.clone()),
                    });

                let xcm_msg = make_xcm_lateral_transfer_native(
                    origin.clone(),
                    params.target_chain_id,
                    params.from, params.amount);

                let xcm_msg_v2 = Xcm::<>::from(xcm_msg);

                XcmExecutor::<XcmConfig>::execute_xcm(origin, xcm_msg_v2, 1_000_000_000)
                    .ensure_complete()
                    .map_err(|_| DispatchError::Other("ChainExtension failed to call total_supply"))?
            }
            1102 => {
                log::info!("chain extension call 1102 transfer");
                let mut env = env.buf_in_buf_out();
                let caller = env.ext().caller();
                let who = to_account_id(caller.as_ref());
                let data = env.read(mem::size_of::<TransferParameter>() as u32).unwrap_or_default();
                let params = TransferParameter::decode(&mut &data[..])
                    .map_err(|_| DispatchError::Other("ChainExtension failed to call balance of"))?;
                log::info!("************* chain extension call 1102 transfer params:  who {:#?} :{:#?}",who,  params);
                CustomMultiAsset::<AccountId, AssetId, Balances, Tokens>::multi_asset_transfer(&params.asset_id, who.borrow(), &params.to, params.amount)?;
            }
            1104 =>{
                log::info!("chain extension call 1104 balance_of");
                let mut env = env.buf_in_buf_out();
                let data = env.read(mem::size_of::<BalancesOfParameter>() as u32).unwrap_or_default();
                let balance = BalancesOfParameter::decode(&mut &data[..]).map_or(
                    Zero::zero(),
                    |params|{
                        CustomMultiAsset::<AccountId, AssetId, Balances, Tokens>::multi_asset_balance_of(&params.asset_id, params.owner.borrow())
                    });
                let balance = balance.encode();
                log::info!("************* chain extension call 1104 balance_of {:#?}", balance);
                env.write(&balance, false, None)
                    .map_err(|_| DispatchError::Other("ChainExtension failed to call balance of"))?;
            }
            1105 =>{
                log::info!("chain extension call 1105 total_supply");
                let mut env = env.buf_in_buf_out();
                let data = env.read(mem::size_of::<TransferParameter>() as u32).unwrap_or_default();
                let total_supply = AssetId::decode(&mut &data[..]).map_or(
                    Zero::zero(),
                    |asset_id|{
                        CustomMultiAsset::<AccountId, AssetId, Balances, Tokens>::multi_asset_total_supply(&asset_id)
                    });
                let total_supply_encode = total_supply.encode();
                env.write(&total_supply_encode, false, None)
                    .map_err(|_| DispatchError::Other("ChainExtension failed to call total_supply"))?;
            }
            _ => {
                log::info!("unknown chain extension");
                return Err(DispatchError::Other("unknown chain extension"));
            }
        };
        Ok(RetVal::Converging(0))
    }

    fn enabled() -> bool {
        true
    }
}

pub fn make_xcm_lateral_transfer_native(
    _location: MultiLocation,
    para_id: u32,
    account: AccountId32,
    amount: u128,
) -> Xcm<()> {
    Xcm::WithdrawAsset {
        assets: vec![MultiAsset::ConcreteFungible { id: MultiLocation::X1(Junction::Parent), amount }],
        effects: vec![
            Order::BuyExecution { fees: MultiAsset::All, weight: 0, debt: 5000, halt_on_error: true, xcm: vec![] },
            Order::DepositReserveAsset {
                assets: vec![MultiAsset::All],
                dest: MultiLocation::X2(
                    Junction::Parent,
                    Junction::Parachain { id: para_id.into() },
                ),
                effects: vec![
                    Order::BuyExecution { fees: MultiAsset::All, weight: 0, debt: 5000, halt_on_error: true, xcm: vec![] },
                    make_deposit_asset_order(account)
                ],
            }],
    }
}

fn make_deposit_asset_order(account: AccountId32) -> Order<()> {
    Order::DepositAsset {
        assets: vec![MultiAsset::All],
        dest: MultiLocation::X1(Junction::AccountId32 {
            network: NetworkId::Any,
            id: <[u8; 32]>::from(account),
        }),
    }
}