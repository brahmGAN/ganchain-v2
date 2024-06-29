// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2015-2022 Parity Technologies (UK) Ltd.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::{
	collections::{BTreeMap, HashSet},
	sync::{Arc, Mutex},
};

use ethereum_types::{Bloom, BloomInput, H160, H256, U256};
use serde::{
	de::{DeserializeOwned, Error},
	Deserialize, Deserializer, Serialize, Serializer,
};
use serde_json::{from_value, Value};

use crate::types::{BlockNumber, Log};

/// Variadic value
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum VariadicValue<T>
where
	T: DeserializeOwned,
{
	/// Single
	Single(T),
	/// List
	Multiple(Vec<T>),
	/// None
	Null,
}

impl<'a, T> Deserialize<'a> for VariadicValue<T>
where
	T: DeserializeOwned,
{
	fn deserialize<D>(deserializer: D) -> Result<VariadicValue<T>, D::Error>
	where
		D: Deserializer<'a>,
	{
		let v: Value = Deserialize::deserialize(deserializer)?;

		if v.is_null() {
			return Ok(VariadicValue::Null);
		}

		from_value(v.clone())
			.map(VariadicValue::Single)
			.or_else(|_| from_value(v).map(VariadicValue::Multiple))
			.map_err(|err| D::Error::custom(format!("Invalid variadic value type: {}", err)))
	}
}

/// Filter Address
pub type FilterAddress = VariadicValue<H160>;
/// Topic, supports `A` | `null` | `[A,B,C]` | `[A,[B,C]]` | `[null,[B,C]]` | `[null,[null,C]]`
pub type Topic = VariadicValue<Option<VariadicValue<Option<H256>>>>;
/// FlatTopic, simplifies the matching logic.
pub type FlatTopic = VariadicValue<Option<H256>>;

pub type BloomFilter<'a> = Vec<Option<Bloom>>;

impl From<&VariadicValue<H160>> for Vec<Option<Bloom>> {
	fn from(address: &VariadicValue<H160>) -> Self {
		let mut blooms = BloomFilter::new();
		match address {
			VariadicValue::Single(address) => {
				let bloom: Bloom = BloomInput::Raw(address.as_ref()).into();
				blooms.push(Some(bloom))
			}
			VariadicValue::Multiple(addresses) => {
				if addresses.is_empty() {
					blooms.push(None);
				} else {
					for address in addresses.iter() {
						let bloom: Bloom = BloomInput::Raw(address.as_ref()).into();
						blooms.push(Some(bloom));
					}
				}
			}
			_ => blooms.push(None),
		}
		blooms
	}
}

impl From<&VariadicValue<Option<H256>>> for Vec<Option<Bloom>> {
	fn from(topics: &VariadicValue<Option<H256>>) -> Self {
		let mut blooms = BloomFilter::new();
		match topics {
			VariadicValue::Single(topic) => {
				if let Some(topic) = topic {
					let bloom: Bloom = BloomInput::Raw(topic.as_ref()).into();
					blooms.push(Some(bloom));
				} else {
					blooms.push(None);
				}
			}
			VariadicValue::Multiple(topics) => {
				if topics.is_empty() {
					blooms.push(None);
				} else {
					for topic in topics.iter() {
						if let Some(topic) = topic {
							let bloom: Bloom = BloomInput::Raw(topic.as_ref()).into();
							blooms.push(Some(bloom));
						} else {
							blooms.push(None);
						}
					}
				}
			}
			_ => blooms.push(None),
		}
		blooms
	}
}

/// Filter
#[derive(Clone, Debug, Eq, PartialEq, Hash, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
	/// From Block
	pub from_block: Option<BlockNumber>,
	/// To Block
	pub to_block: Option<BlockNumber>,
	/// Block hash
	pub block_hash: Option<H256>,
	/// Address
	pub address: Option<FilterAddress>,
	/// Topics
	pub topics: Option<Topic>,
}

/// Helper for Filter matching.
/// Supports conditional indexed parameters and wildcards.
#[derive(Debug, Default)]
pub struct FilteredParams {
	pub filter: Option<Filter>,
	pub flat_topics: Vec<FlatTopic>,
}

impl FilteredParams {
	pub fn new(f: Option<Filter>) -> Self {
		if let Some(f) = f {
			return FilteredParams {
				filter: Some(f.clone()),
				flat_topics: {
					if let Some(t) = f.topics {
						Self::flatten(&t)
					} else {
						Vec::new()
					}
				},
			};
		}
		Self::default()
	}

	/// Build an address-based BloomFilter.
	pub fn adresses_bloom_filter(address: &Option<FilterAddress>) -> BloomFilter<'_> {
		if let Some(address) = address {
			return address.into();
		}
		Vec::new()
	}

	/// Build a topic-based BloomFilter.
	pub fn topics_bloom_filter(topics: &Option<Vec<FlatTopic>>) -> Vec<BloomFilter<'_>> {
		let mut output: Vec<BloomFilter> = Vec::new();
		if let Some(topics) = topics {
			for flat in topics {
				output.push(flat.into());
			}
		}
		output
	}

	/// Evaluates if a Bloom contains a provided sequence of topics.
	pub fn topics_in_bloom(bloom: Bloom, topic_bloom_filters: &[BloomFilter]) -> bool {
		if topic_bloom_filters.is_empty() {
			// No filter provided, match.
			return true;
		}
		// A logical OR evaluation over `topic_bloom_filters`.
		for subset in topic_bloom_filters.iter() {
			let mut matches = false;
			for el in subset {
				matches = match el {
					Some(input) => bloom.contains_bloom(input),
					// Wildcards are true.
					None => true,
				};
				// Each subset must be evaluated sequentially to true or break.
				if !matches {
					break;
				}
			}
			// If any subset is fully evaluated to true, there is no further evaluation.
			if matches {
				return true;
			}
		}
		false
	}

	/// Evaluates if a Bloom contains the provided address(es).
	pub fn address_in_bloom(bloom: Bloom, address_bloom_filter: &BloomFilter) -> bool {
		if address_bloom_filter.is_empty() {
			// No filter provided, match.
			return true;
		} else {
			// Wildcards are true.
			for el in address_bloom_filter {
				if match el {
					Some(input) => bloom.contains_bloom(input),
					None => true,
				} {
					return true;
				}
			}
		}
		false
	}

	/// Cartesian product for VariadicValue conditional indexed parameters.
	/// Executed once on struct instance.
	/// i.e. `[A,[B,C]]` to `[[A,B],[A,C]]`.
	fn flatten(topic: &Topic) -> Vec<FlatTopic> {
		fn cartesian(lists: &[Vec<Option<H256>>]) -> Vec<Vec<Option<H256>>> {
			let mut res = vec![];
			let mut list_iter = lists.iter();
			if let Some(first_list) = list_iter.next() {
				for &i in first_list {
					res.push(vec![i]);
				}
			}
			for l in list_iter {
				let mut tmp = vec![];
				for r in res {
					for &el in l {
						let mut tmp_el = r.clone();
						tmp_el.push(el);
						tmp.push(tmp_el);
					}
				}
				res = tmp;
			}
			res
		}
		let mut out: Vec<FlatTopic> = Vec::new();
		match topic {
			VariadicValue::Multiple(multi) => {
				let mut values: Vec<Vec<Option<H256>>> = Vec::new();
				for v in multi {
					values.push({
						if let Some(v) = v {
							match v {
								VariadicValue::Single(s) => {
									vec![*s]
								}
								VariadicValue::Multiple(s) => s.clone(),
								VariadicValue::Null => {
									vec![None]
								}
							}
						} else {
							vec![None]
						}
					});
				}
				for permut in cartesian(&values) {
					out.push(FlatTopic::Multiple(permut));
				}
			}
			VariadicValue::Single(single) => {
				if let Some(single) = single {
					out.push(single.clone());
				}
			}
			VariadicValue::Null => {
				out.push(FlatTopic::Null);
			}
		}
		out
	}

	/// Replace None values - aka wildcards - for the log input value in that position.
	pub fn replace(&self, log: &Log, topic: FlatTopic) -> Option<Vec<H256>> {
		let mut out: Vec<H256> = Vec::new();
		match topic {
			VariadicValue::Single(Some(value)) => {
				out.push(value);
			}
			VariadicValue::Multiple(value) => {
				for (k, v) in value.into_iter().enumerate() {
					if let Some(v) = v {
						out.push(v);
					} else {
						out.push(log.topics[k]);
					}
				}
			}
			_ => {}
		};
		if out.is_empty() {
			return None;
		}
		Some(out)
	}

	pub fn filter_block_range(&self, block_number: u64) -> bool {
		let mut out = true;
		let filter = self.filter.clone().unwrap();
		if let Some(BlockNumber::Num(from)) = filter.from_block {
			if from > block_number {
				out = false;
			}
		}
		if let Some(to) = filter.to_block {
			match to {
				BlockNumber::Num(to) => {
					if to < block_number {
						out = false;
					}
				}
				BlockNumber::Earliest => {
					out = false;
				}
				_ => {}
			}
		}
		out
	}

	pub fn filter_block_hash(&self, block_hash: H256) -> bool {
		if let Some(h) = self.filter.clone().unwrap().block_hash {
			if h != block_hash {
				return false;
			}
		}
		true
	}

	pub fn filter_address(&self, log: &Log) -> bool {
		if let Some(input_address) = &self.filter.clone().unwrap().address {
			match input_address {
				VariadicValue::Single(x) => {
					if log.address != *x {
						return false;
					}
				}
				VariadicValue::Multiple(x) => {
					if !x.contains(&log.address) {
						return false;
					}
				}
				_ => {
					return true;
				}
			}
		}
		true
	}

	pub fn filter_topics(&self, log: &Log) -> bool {
		let mut out: bool = true;
		for topic in self.flat_topics.clone() {
			match topic {
				VariadicValue::Single(single) => {
					if let Some(single) = single {
						if !log.topics.starts_with(&[single]) {
							out = false;
						}
					}
				}
				VariadicValue::Multiple(multi) => {
					// Shrink the topics until the last item is Some.
					let mut new_multi = multi;
					while new_multi
						.iter()
						.last()
						.unwrap_or(&Some(H256::default()))
						.is_none()
					{
						new_multi.pop();
					}
					// We can discard right away any logs with lesser topics than the filter.
					if new_multi.len() > log.topics.len() {
						out = false;
						break;
					}
					let replaced: Option<Vec<H256>> =
						self.replace(log, VariadicValue::Multiple(new_multi));
					if let Some(replaced) = replaced {
						out = false;
						if log.topics.starts_with(&replaced[..]) {
							out = true;
							break;
						}
					}
				}
				_ => {
					out = true;
				}
			}
		}
		out
	}
}

/// Results of the filter_changes RPC.
#[derive(Debug, Eq, PartialEq)]
pub enum FilterChanges {
	/// New logs.
	Logs(Vec<Log>),
	/// New hashes (block or transactions)
	Hashes(Vec<H256>),
	/// Empty result,
	Empty,
}

impl Serialize for FilterChanges {
	fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		match *self {
			FilterChanges::Logs(ref logs) => logs.serialize(s),
			FilterChanges::Hashes(ref hashes) => hashes.serialize(s),
			FilterChanges::Empty => (&[] as &[Value]).serialize(s),
		}
	}
}

#[derive(Clone, Debug)]
pub enum FilterType {
	Block,
	PendingTransaction,
	Log(Filter),
}

#[derive(Clone, Debug)]
pub struct FilterPoolItem {
	pub last_poll: BlockNumber,
	pub filter_type: FilterType,
	pub at_block: u64,
	pub pending_transaction_hashes: HashSet<H256>,
}

/// On-memory stored filters created through the `eth_newFilter` RPC.
pub type FilterPool = Arc<Mutex<BTreeMap<U256, FilterPoolItem>>>;

#[cfg(test)]
mod tests {
	use super::*;
	use std::str::FromStr;

	fn block_bloom() -> Bloom {
		let test_address = H160::from_str("1000000000000000000000000000000000000000").unwrap();
		let topic1 =
			H256::from_str("1000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let topic2 =
			H256::from_str("2000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();

		let mut block_bloom = Bloom::default();
		block_bloom.accrue(BloomInput::Raw(&test_address[..]));
		block_bloom.accrue(BloomInput::Raw(&topic1[..]));
		block_bloom.accrue(BloomInput::Raw(&topic2[..]));
		block_bloom
	}

	#[test]
	fn bloom_filter_should_match_by_address() {
		let test_address = H160::from_str("1000000000000000000000000000000000000000").unwrap();
		let filter = Filter {
			from_block: None,
			to_block: None,
			block_hash: None,
			address: Some(VariadicValue::Single(test_address)),
			topics: None,
		};
		let address_bloom = FilteredParams::adresses_bloom_filter(&filter.address);
		assert!(FilteredParams::address_in_bloom(
			block_bloom(),
			&address_bloom
		));
	}

	#[test]
	fn bloom_filter_should_not_match_by_address() {
		let test_address = H160::from_str("2000000000000000000000000000000000000000").unwrap();
		let filter = Filter {
			from_block: None,
			to_block: None,
			block_hash: None,
			address: Some(VariadicValue::Single(test_address)),
			topics: None,
		};
		let address_bloom = FilteredParams::adresses_bloom_filter(&filter.address);
		assert!(!FilteredParams::address_in_bloom(
			block_bloom(),
			&address_bloom
		));
	}
	#[test]
	fn bloom_filter_should_match_by_topic() {
		let topic1 =
			H256::from_str("1000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let topic2 =
			H256::from_str("2000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let topic3 =
			H256::from_str("3000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let filter = Filter {
			from_block: None,
			to_block: None,
			block_hash: None,
			address: None,
			topics: Some(VariadicValue::Multiple(vec![
				Some(VariadicValue::Single(Some(topic1))),
				Some(VariadicValue::Multiple(vec![Some(topic2), Some(topic3)])),
			])),
		};
		let topics_input = if let Some(_) = &filter.topics {
			let filtered_params = FilteredParams::new(Some(filter.clone()));
			Some(filtered_params.flat_topics)
		} else {
			None
		};
		let topics_bloom = FilteredParams::topics_bloom_filter(&topics_input);
		assert!(FilteredParams::topics_in_bloom(
			block_bloom(),
			&topics_bloom
		));
	}
	#[test]
	fn bloom_filter_should_not_match_by_topic() {
		let topic1 =
			H256::from_str("1000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let topic2 =
			H256::from_str("4000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let topic3 =
			H256::from_str("5000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let filter = Filter {
			from_block: None,
			to_block: None,
			block_hash: None,
			address: None,
			topics: Some(VariadicValue::Multiple(vec![
				Some(VariadicValue::Single(Some(topic1))),
				Some(VariadicValue::Multiple(vec![Some(topic2), Some(topic3)])),
			])),
		};
		let topics_input = if let Some(_) = &filter.topics {
			let filtered_params = FilteredParams::new(Some(filter.clone()));
			Some(filtered_params.flat_topics)
		} else {
			None
		};
		let topics_bloom = FilteredParams::topics_bloom_filter(&topics_input);
		assert!(!FilteredParams::topics_in_bloom(
			block_bloom(),
			&topics_bloom
		));
	}
	#[test]
	fn bloom_filter_should_match_by_empty_topic() {
		let filter = Filter {
			from_block: None,
			to_block: None,
			block_hash: None,
			address: None,
			topics: Some(VariadicValue::Multiple(vec![])),
		};
		let topics_input = if let Some(_) = &filter.topics {
			let filtered_params = FilteredParams::new(Some(filter.clone()));
			Some(filtered_params.flat_topics)
		} else {
			None
		};
		let topics_bloom = FilteredParams::topics_bloom_filter(&topics_input);
		assert!(FilteredParams::topics_in_bloom(
			block_bloom(),
			&topics_bloom
		));
	}
	#[test]
	fn bloom_filter_should_match_combined() {
		let test_address = H160::from_str("1000000000000000000000000000000000000000").unwrap();
		let topic1 =
			H256::from_str("1000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let topic2 =
			H256::from_str("2000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let topic3 =
			H256::from_str("3000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let filter = Filter {
			from_block: None,
			to_block: None,
			block_hash: None,
			address: Some(VariadicValue::Single(test_address)),
			topics: Some(VariadicValue::Multiple(vec![
				Some(VariadicValue::Single(Some(topic1))),
				Some(VariadicValue::Multiple(vec![Some(topic2), Some(topic3)])),
			])),
		};
		let topics_input = if let Some(_) = &filter.topics {
			let filtered_params = FilteredParams::new(Some(filter.clone()));
			Some(filtered_params.flat_topics)
		} else {
			None
		};
		let address_bloom = FilteredParams::adresses_bloom_filter(&filter.address);
		let topics_bloom = FilteredParams::topics_bloom_filter(&topics_input);
		let matches = FilteredParams::address_in_bloom(block_bloom(), &address_bloom)
			&& FilteredParams::topics_in_bloom(block_bloom(), &topics_bloom);
		assert!(matches);
	}
	#[test]
	fn bloom_filter_should_not_match_combined() {
		let test_address = H160::from_str("2000000000000000000000000000000000000000").unwrap();
		let topic1 =
			H256::from_str("1000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let topic2 =
			H256::from_str("2000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let topic3 =
			H256::from_str("3000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let filter = Filter {
			from_block: None,
			to_block: None,
			block_hash: None,
			address: Some(VariadicValue::Single(test_address)),
			topics: Some(VariadicValue::Multiple(vec![
				Some(VariadicValue::Single(Some(topic1))),
				Some(VariadicValue::Multiple(vec![Some(topic2), Some(topic3)])),
			])),
		};
		let topics_input = if let Some(_) = &filter.topics {
			let filtered_params = FilteredParams::new(Some(filter.clone()));
			Some(filtered_params.flat_topics)
		} else {
			None
		};
		let address_bloom = FilteredParams::adresses_bloom_filter(&filter.address);
		let topics_bloom = FilteredParams::topics_bloom_filter(&topics_input);
		let matches = FilteredParams::address_in_bloom(block_bloom(), &address_bloom)
			&& FilteredParams::topics_in_bloom(block_bloom(), &topics_bloom);
		assert!(!matches);
	}
	#[test]
	fn bloom_filter_should_match_wildcards_by_topic() {
		let topic2 =
			H256::from_str("2000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let topic3 =
			H256::from_str("3000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let filter = Filter {
			from_block: None,
			to_block: None,
			block_hash: None,
			address: None,
			topics: Some(VariadicValue::Multiple(vec![
				None,
				Some(VariadicValue::Multiple(vec![Some(topic2), Some(topic3)])),
			])),
		};
		let topics_input = if let Some(_) = &filter.topics {
			let filtered_params = FilteredParams::new(Some(filter.clone()));
			Some(filtered_params.flat_topics)
		} else {
			None
		};
		let topics_bloom = FilteredParams::topics_bloom_filter(&topics_input);
		assert!(FilteredParams::topics_in_bloom(
			block_bloom(),
			&topics_bloom
		));
	}
	#[test]
	fn bloom_filter_should_not_match_wildcards_by_topic() {
		let topic2 =
			H256::from_str("4000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let topic3 =
			H256::from_str("5000000000000000000000000000000000000000000000000000000000000000")
				.unwrap();
		let filter = Filter {
			from_block: None,
			to_block: None,
			block_hash: None,
			address: None,
			topics: Some(VariadicValue::Multiple(vec![
				None,
				Some(VariadicValue::Multiple(vec![Some(topic2), Some(topic3)])),
			])),
		};
		let topics_input = if let Some(_) = &filter.topics {
			let filtered_params = FilteredParams::new(Some(filter.clone()));
			Some(filtered_params.flat_topics)
		} else {
			None
		};
		let topics_bloom = FilteredParams::topics_bloom_filter(&topics_input);
		assert!(!FilteredParams::topics_in_bloom(
			block_bloom(),
			&topics_bloom
		));
	}
}
