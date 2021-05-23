// A demo Asset adapter used to transfer asset between contract and runtime or parachain.
use codec::{Encode, Decode};

use sp_std::{marker::PhantomData, convert::TryFrom, fmt::Debug};
use sp_runtime::{DispatchError, traits::Zero};
use frame_support::{dispatch::DispatchResult, traits::{ExistenceRequirement, Currency}};

use orml_traits::MultiCurrency;

use rococo_parachain_primitives::{TokenSymbol, CurrencyId, AccountId};
use crate::{Address, Balances, Tokens, ParachainInfo};
use xcm_executor::traits::TransactAsset;
use xcm::v0::{MultiAsset, MultiLocation, Error as XcmError, Junction};
use xcm_executor::Assets;
use super::Contracts;
use sp_std::vec;
use crate::contract_assets_adapter::AssetId::{Module, Local};


type TokenBalance = u128;

// Mark where the asset is issued
#[derive(Debug, Encode, Decode)]
pub enum AssetProducer {
    PALLET(u8),
    CONTRACT(Address),
}

#[derive(Debug, Encode, Decode)]
pub enum AssetId {
    // asset in module
    Module(u32, u8, u32),
    // contract asset form local chain, like local erc20 contract.
    Local(AccountId),
}

pub trait CustomMultiAssetAdapter<AccountId, AssetId, Balances, Tokens> {
    fn multi_asset_total_supply(asset_id: &AssetId) -> TokenBalance;

    fn multi_asset_balance_of(asset_id: &AssetId, who: &AccountId) -> TokenBalance;

    fn multi_asset_transfer(asset_id: &AssetId, from: &AccountId, to: &AccountId, amount: TokenBalance) -> DispatchResult;
}

pub struct CustomMultiAsset<AccountId, AssetId, Balances, Tokens>(
    PhantomData<(AccountId, AssetId, Balances, Tokens)>
);

// Process asset in signal chain.
impl CustomMultiAssetAdapter<AccountId, AssetId, Balances, Tokens> for
CustomMultiAsset<AccountId, AssetId, Balances, Tokens>
    where
        AccountId: Debug,
        AssetId: Debug,
        Balances: frame_support::traits::Currency<AccountId>,
        Tokens: MultiCurrency<AccountId>
{
    fn multi_asset_total_supply(asset_id: &AssetId) -> TokenBalance {
        log::info!("chain_extension multi_asset_total_supply assetId {:#?}", asset_id);
        match asset_id {
            Module(chain_id, module_index, asset_index) => {
                let para_id: u32 = ParachainInfo::parachain_id().into();
                if para_id != *chain_id {
                    return Zero::zero();
                }
                if *module_index == 1 {
                    return Balances::total_issuance();
                }
                if *module_index == 2 {
                    return TokenSymbol::try_from(*asset_index as u8).map_or(
                        Zero::zero(),
                        |symbol| {
                            Tokens::total_issuance(CurrencyId::Token(symbol))
                        },
                    );
                }
                Zero::zero()
            }
            Local(..)=>{
                unimplemented!()
            }
        }
    }

    fn multi_asset_balance_of(asset_id: &AssetId, who: &AccountId) -> TokenBalance {
        log::info!("chain_extension multi_asset_balance_of balance_of {:#?}", asset_id);
        match asset_id {
            Module(chain_id, module_index, asset_index) => {
                let para_id: u32 = ParachainInfo::parachain_id().into();
                if para_id != *chain_id {
                    return Zero::zero();
                }
                if *module_index == 1 {
                    return Balances::free_balance(who);
                }
                if *module_index == 2 {
                    return TokenSymbol::try_from(*asset_index as u8).map_or(
                        Zero::zero(),
                        |symbol| {
                            Tokens::free_balance(CurrencyId::Token(symbol), who)
                        },
                    );
                }
                Zero::zero()
            }
            Local(..) =>{
                unimplemented!()
            }
        }
    }

    fn multi_asset_transfer(
        asset_id: &AssetId,
        from: &AccountId,
        to: &AccountId,
        amount: TokenBalance,
    ) -> DispatchResult {
        match asset_id {
            Module(chain_id, module_index, asset_index) => {
                let para_id: u32 = ParachainInfo::parachain_id().into();
                if para_id != *chain_id {
                    return Err(DispatchError::Other("unknown asset id"));
                }
                if *module_index == 1 {
                    log::info!("chain_extension multi_asset_transfer balances");
                    return <Balances as Currency<AccountId>>::transfer(from, to, amount, ExistenceRequirement::KeepAlive);
                }
                if *module_index == 2 {
                    log::info!("chain_extension multi_asset_transfer tokens");
                    return TokenSymbol::try_from(*asset_index as u8).map_or(
                        Err(DispatchError::Other("unknown asset id")),
                        |symbol| {
                            <Tokens as MultiCurrency<AccountId>>::transfer(CurrencyId::Token(symbol), from, to, amount)
                        },
                    );
                }
                Err(DispatchError::Other("unknown asset id"))
            }
            Local(address)=>{
                unimplemented!()
            }
        }
    }
}

impl TransactAsset for CustomMultiAsset<AccountId, AssetId, Balances, Tokens>
    where
        AccountId: Debug,
        AssetId: Debug,
        Balances: frame_support::traits::Currency<AccountId>,
        Tokens: MultiCurrency<AccountId>
{
    fn deposit_asset(what: &MultiAsset, who: &MultiLocation) -> Result<(), XcmError> {
        // Check we handle this asset.
        log::info!("CustomMultiAsset::deposit asset what{:#?} who{:#?} \n", what, who);
        let from = match who {
            MultiLocation::X1(Junction::AccountId32 { network, id }) => {
                Ok(id)
            }
            _ => Err(())
        }.map_err(|_| XcmError::LocationCannotHold)?;

        match what {
            MultiAsset::ConcreteFungible { id: location, amount } => {
                match location {
                    MultiLocation::X3(Junction::Parachain { id: para_id },
                                      Junction::AccountId32 { network: _, id: address },
                                      Junction::GeneralIndex { id: asset_index }) => {
                        //call contract deposit ?
                        // let selector = "0xbdd16bfa".encode();
                        // let asset_encode = asset.encode();
                        // let input_data = [&selector[..], &from[..], &asset_encode[..]].concat();
                        // Contracts::bare_call((), (), (), 0, input_data);
                        unimplemented!()
                    }
                    MultiLocation::X3(Junction::Parachain { id: para_id },
                                      Junction::PalletInstance(pallet_index),
                                      Junction::GeneralIndex { id: asset_index }) => {

                        //call pallet deposit
                        unimplemented!()
                    }
                    _ => {}
                };
                Ok(())
            }
            _ => {
                Err(XcmError::Unimplemented)
            }
        }
    }

    fn withdraw_asset(
        what: &MultiAsset,
        who: &MultiLocation,
    ) -> Result<Assets, XcmError> {
        // Check we handle this asset.
        log::info!("CustomMultiAsset::deposit asset what{:#?} who{:#?} \n", what, who);
        unimplemented!();
        Ok(what.clone().into())
    }
}