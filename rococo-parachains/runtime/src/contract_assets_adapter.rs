// A demo Asset adapter used to transfer asset between contract and runtime or parachain.
use codec::{Encode, Decode};

use sp_std::{ marker::PhantomData, convert::TryFrom, fmt::Debug};
use sp_runtime::{DispatchError, traits::Zero};
use frame_support::{dispatch::DispatchResult ,traits::{ExistenceRequirement, Currency}};

use orml_traits::MultiCurrency;

use rococo_parachain_primitives::{AccountId, TokenSymbol, CurrencyId};
use crate::{Address, Balances, Tokens, ParachainInfo};


type TokenBalance = u128;

// Mark where the asset is issued
#[derive(Debug, Encode, Decode)]
pub enum AssetProducer {
    PALLET(u8),
    CONTRACT(Address),
}

#[derive(Debug, Encode, Decode)]
pub struct AssetId {
    pub chain_id: u32,
    pub producer: AssetProducer,
    pub asset_index: u32,
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
        let para_id: u32 = ParachainInfo::parachain_id().into();
        if para_id != asset_id.chain_id {
            return Zero::zero();
        }
        match asset_id.producer {
            AssetProducer::PALLET(pallet_index) => {
                if pallet_index == 1 {
                    return Balances::total_issuance();
                }
                if pallet_index == 2 {
                    return TokenSymbol::try_from(asset_id.asset_index as u8).map_or(
                        Zero::zero(),
                        |symbol| {
                            Tokens::total_issuance(CurrencyId::Token(symbol))
                        },
                    );
                }
                Zero::zero()
            }
            AssetProducer::CONTRACT(_) => Zero::zero()
        }
    }

    fn multi_asset_balance_of(asset_id: &AssetId, who: &AccountId) -> TokenBalance {
        let para_id: u32 = ParachainInfo::parachain_id().into();
        if para_id != asset_id.chain_id {
            return Zero::zero();
        }
        match asset_id.producer {
            AssetProducer::PALLET(pallet_index) => {
                if pallet_index == 1 {
                    return Balances::free_balance(who);
                }
                if pallet_index == 2 {
                    return TokenSymbol::try_from(asset_id.asset_index as u8).map_or(
                        Zero::zero(),
                        |symbol| {
                            Tokens::free_balance(CurrencyId::Token(symbol), who)
                        },
                    );
                }
                Zero::zero()
            }
            AssetProducer::CONTRACT(_) => Zero::zero()
        }
    }

    fn multi_asset_transfer(
        asset_id: &AssetId,
        from: &AccountId,
        to: &AccountId,
        amount: TokenBalance,
    ) -> DispatchResult {
        let para_id: u32 = ParachainInfo::parachain_id().into();
        if para_id != asset_id.chain_id {
            return Err(DispatchError::Other("unknown asset id"));
        }
        match asset_id.producer {
            AssetProducer::PALLET(pallet_index) => {
                if pallet_index == 1 {
                    return <Balances as Currency<AccountId>>::transfer(from, to, amount, ExistenceRequirement::KeepAlive);
                }
                if pallet_index == 2 {
                    return TokenSymbol::try_from(asset_id.asset_index as u8).map_or(
                        Err(DispatchError::Other("unknown asset id")),
                        |symbol| {
                            <Tokens as MultiCurrency<AccountId>>::transfer(CurrencyId::Token(symbol), from, to, amount)
                        },
                    );
                }
                Err(DispatchError::Other("unknown asset id"))
            }
            AssetProducer::CONTRACT(_) => Err(DispatchError::Other("known asset"))
        }
    }
}