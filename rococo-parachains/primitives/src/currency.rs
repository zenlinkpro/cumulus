// This file is part of Acala.

// Copyright (C) 2020-2021 Acala Foundation.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

#![allow(clippy::from_over_into)]

use bstringify::bstringify;
use codec::{Decode, Encode};
use sp_runtime::RuntimeDebug;
use sp_std::{
    convert::{Into, TryFrom},
    prelude::*,
};

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

macro_rules! create_currency_id {
    ($(#[$meta:meta])*
	$vis:vis enum TokenSymbol {
        $($(#[$vmeta:meta])* $symbol:ident($name:expr, $deci:literal) = $val:literal,)*
    }) => {
		$(#[$meta])*
		$vis enum TokenSymbol {
			$($(#[$vmeta])* $symbol = $val,)*
		}

		impl TryFrom<u8> for TokenSymbol {
			type Error = ();

			fn try_from(v: u8) -> Result<Self, Self::Error> {
				match v {
					$($val => Ok(TokenSymbol::$symbol),)*
					_ => Err(()),
				}
			}
		}

		impl Into<u8> for TokenSymbol {
			fn into(self) -> u8 {
				match self {
					$(TokenSymbol::$symbol => ($val),)*
				}
			}
		}

		impl TryFrom<Vec<u8>> for CurrencyId {
			type Error = ();
			fn try_from(v: Vec<u8>) -> Result<CurrencyId, ()> {
				match v.as_slice() {
					$(bstringify!($symbol) => Ok(CurrencyId::Token(TokenSymbol::$symbol)),)*
					_ => Err(()),
				}
			}
		}

		impl TokenInfo for CurrencyId {
			fn currency_id(&self) -> Option<u8> {
				match self {
					$(CurrencyId::Token(TokenSymbol::$symbol) => Some($val),)*
				}
			}
			fn name(&self) -> Option<&str> {
				match self {
					$(CurrencyId::Token(TokenSymbol::$symbol) => Some($name),)*
				}
			}
			fn symbol(&self) -> Option<&str> {
				match self {
					$(CurrencyId::Token(TokenSymbol::$symbol) => Some(stringify!($symbol)),)*
				}
			}
			fn decimals(&self) -> Option<u8> {
				match self {
					$(CurrencyId::Token(TokenSymbol::$symbol) => Some($deci),)*
				}
			}
		}

		$(pub const $symbol: CurrencyId = CurrencyId::Token(TokenSymbol::$symbol);)*

		impl TokenSymbol {
			pub fn get_info() -> Vec<(&'static str, u32)> {
				vec![
					$((stringify!($symbol), $deci),)*
				]
			}
		}
    }
}

create_currency_id! {
	// Represent a Token symbol with 8 bit
	// Bit 8 : 0 for Pokladot Ecosystem, 1 for Kusama Ecosystem
	// Bit 7 : Reserved
	// Bit 6 - 1 : The token ID
	#[derive(Encode, Decode, Eq, PartialEq, Copy, Clone, RuntimeDebug, PartialOrd, Ord)]
	#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
	#[repr(u8)]
	pub enum TokenSymbol {
		// Polkadot Ecosystem
		// ACA("Acala", 12) = 0,
		// AUSD("Acala Dollar", 12) = 1,
		DOT("Polkadot", 10) = 2,
		// LDOT("Liquid DOT", 10) = 3,
		// RENBTC("Ren Protocol BTC", 8) = 4,
		//
		// // Kusama Ecosystem
		// KAR("Karura", 12) = 128,
		// KUSD("Karura Dollar", 12) = 129,
		// KSM("Kusama", 12) = 130,
		// LKSM("Liquid KSM", 12) = 131,
		// Reserve for RENBTC = 132
	}
}

pub trait TokenInfo {
    fn currency_id(&self) -> Option<u8>;
    fn name(&self) -> Option<&str>;
    fn symbol(&self) -> Option<&str>;
    fn decimals(&self) -> Option<u8>;
}

#[derive(Encode, Decode, Eq, PartialEq, Copy, Clone, RuntimeDebug, PartialOrd, Ord)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum DexShare {
    Token(TokenSymbol),
}

#[derive(Encode, Decode, Eq, PartialEq, Copy, Clone, RuntimeDebug, PartialOrd, Ord)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum CurrencyId {
    Token(TokenSymbol),
}

impl CurrencyId {
    pub fn is_token_currency_id(&self) -> bool {
        matches!(self, CurrencyId::Token(_))
    }
}