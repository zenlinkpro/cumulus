use pallet_contracts::chain_extension::{
    ChainExtension, Environment, Ext, InitState, RetVal, SysConfig, UncheckedFrom,
};
use sp_runtime::{DispatchError, AccountId32};
use super::Runtime;
use sp_std::mem;
use codec::{Encode, Decode};
use xcm_executor::XcmExecutor;
use crate::{XcmConfig, ParachainInfo};
use xcm::v0::{MultiLocation, Junction, NetworkId, MultiAsset, Xcm, Order, ExecuteXcm};
use sp_std::vec;
use frame_support::traits::{All, Currency};
use crate::sp_api_hidden_includes_construct_runtime::hidden_include::traits::Get;
use frame_support::sp_runtime::sp_std::default::default;

pub struct XcmSenderExtension;

#[derive(Debug, Encode, Decode)]
struct XcmParameter {
    pub from: AccountId32,
    pub target_chain_id: u32,
    pub amount: u128,
}

// Now, we ignore AssetId, just transfer Balance;
#[derive(Debug, Encode, Decode)]
struct TransferParameter {
    pub from: AccountId32,
    pub to: AccountId32,
    //pub AssetId: asset_id,
    pub amount: u128,
}

impl ChainExtension<Runtime> for XcmSenderExtension {
    fn call<E: Ext>(func_id: u32, mut env: Environment<E, InitState>) -> Result<RetVal, DispatchError>
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

                let origin = MultiLocation::from(
                    Junction::AccountId32 {
                        network: NetworkId::Any,
                        id: <[u8; 32]>::from(params.from.clone()) });

                let xcm_msg = make_xcm_lateral_transfer_native(
                    origin.clone(),
                    params.target_chain_id,
                    params.from, params.amount);

                let xcm_msg_v2 = Xcm::<>::from(xcm_msg);

                let res = XcmExecutor::<XcmConfig>::execute_xcm(origin, xcm_msg_v2, 1_000_000_000);
                log::info!("chain extension step 2 {:#?}", res);
            }
            1102 =>{
                // transfer from module to contract
                log::info!("chain extension call 1102 transfer_from_module_to_contract");
                //step1: get from_account, amount, contract_address, asset_id , to_address from env
                //step2: check balance. asset_module::balance_of(from_address)
                //step3: asset_module::transfer(from_account, contract_address, amount);
                //step4  contract::deposit(to_account, amount)
                let env = env.buf_in_buf_out();
                let data = env.read(mem::size_of::<TransferParameter>() as u32).unwrap_or_default();
                let res = TransferParameter::decode(&mut &data[..]);


            }
            1103 =>{
                log::info!("chain extension call 1103 transfer_from_contract_to_module");
                //step -2: check from_account balance, use call contract::balances_of. //do in contract.
                //step -1: contract::withdraw(from_account)                            //do in contract.

                // The steps above should been done in contract.
                // if contract do something bad, the impact is limited to the contract.

                //step1: get from_account, amount, contract_address, asset_id , to_address from env
                //step2: asset_module::transfer(contract_address, to_account)
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
