// This file is part of Astar.

// Copyright 2019-2022 PureStake Inc.
// Copyright (C) 2022-2023 Stake Technologies Pte.Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Utils package, originally developed by Purestake Inc.
// Utils package used in Astar Network in terms of GPLv3.
//
// Utils is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Utils is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Utils.  If not, see <http://www.gnu.org/licenses/>.

#![crate_type = "proc-macro"]
extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2::Literal;
use quote::{quote, quote_spanned};
use sha3::{Digest, Keccak256};
use syn::{parse_macro_input, spanned::Spanned, Expr, ExprLit, Ident, ItemEnum, Lit, LitStr};

struct Bytes(Vec<u8>);

impl ::std::fmt::Debug for Bytes {
	#[inline]
	fn fmt(&self, f: &mut std::fmt::Formatter) -> ::std::fmt::Result {
		let data = &self.0;
		write!(f, "[")?;
		if !data.is_empty() {
			write!(f, "{:#04x}u8", data[0])?;
			for unit in data.iter().skip(1) {
				write!(f, ", {:#04x}", unit)?;
			}
		}
		write!(f, "]")
	}
}

#[proc_macro]
pub fn keccak256(input: TokenStream) -> TokenStream {
	let lit_str = parse_macro_input!(input as LitStr);

	let hash = Keccak256::digest(lit_str.value().as_bytes());

	let bytes = Bytes(hash.to_vec());
	let eval_str = format!("{:?}", bytes);
	let eval_ts: proc_macro2::TokenStream = eval_str.parse().unwrap_or_else(|_| {
		panic!("Failed to parse the string \"{}\" to TokenStream.", eval_str);
	});
	quote!(#eval_ts).into()
}

/// This macro allows to associate to each variant of an enumeration a discriminant (of type u32
/// whose value corresponds to the first 4 bytes of the Hash Keccak256 of the character string
///indicated by the user of this macro.
///
/// Usage:
///
/// ```ignore
/// #[generate_function_selector]
/// enum Action {
///     Toto = "toto()",
///     Tata = "tata()",
/// }
/// ```
///
/// Extanded to:
///
/// ```rust
/// #[repr(u32)]
/// enum Action {
///     Toto = 119097542u32,
///     Tata = 1414311903u32,
/// }
/// ```
///
#[proc_macro_attribute]
pub fn generate_function_selector(_: TokenStream, input: TokenStream) -> TokenStream {
	let item = parse_macro_input!(input as ItemEnum);

	let ItemEnum { attrs, vis, enum_token, ident, variants, .. } = item;

	let mut ident_expressions: Vec<Ident> = vec![];
	let mut variant_expressions: Vec<Expr> = vec![];
	for variant in variants {
		match variant.discriminant {
			Some((_, Expr::Lit(ExprLit { lit, .. }))) =>
				if let Lit::Str(lit_str) = lit {
					let digest = Keccak256::digest(lit_str.value().as_bytes());
					let selector = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);

					ident_expressions.push(variant.ident);
					variant_expressions.push(Expr::Lit(ExprLit {
						lit: Lit::Verbatim(Literal::u32_suffixed(selector)),
						attrs: Default::default(),
					}));
				} else {
					return quote_spanned! {
						lit.span() => compile_error("Expected literal string");
					}
					.into()
				},
			Some((_eg, expr)) =>
				return quote_spanned! {
					expr.span() => compile_error("Expected literal");
				}
				.into(),
			None =>
				return quote_spanned! {
					variant.span() => compile_error("Each variant must have a discriminant");
				}
				.into(),
		}
	}

	(quote! {
		#(#attrs)*
		#[derive(num_enum::TryFromPrimitive, num_enum::IntoPrimitive)]
		#[repr(u32)]
		#vis #enum_token #ident {
			#(
				#ident_expressions = #variant_expressions,
			)*
		}
	})
	.into()
}
