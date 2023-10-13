use std::ops::Deref;

use crate::table::MPTProofType;

use serde::{Deserialize, Serialize};

use super::RlpItemType;

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum StorageRowType {
    KeyS,
    ValueS,
    KeyC,
    ValueC,
    Drifted,
    Wrong,
    Address,
    Key,
    Count,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum AccountRowType {
    KeyS,
    KeyC,
    NonceS,
    BalanceS,
    StorageS,
    CodehashS,
    NonceC,
    BalanceC,
    StorageC,
    CodehashC,
    Drifted,
    Wrong,
    Address,
    Key,
    Count,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ExtensionBranchRowType {
    Mod,
    Child0,
    Child1,
    Child2,
    Child3,
    Child4,
    Child5,
    Child6,
    Child7,
    Child8,
    Child9,
    Child10,
    Child11,
    Child12,
    Child13,
    Child14,
    Child15,
    KeyS,
    ValueS,
    KeyC,
    ValueC,
    Count,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum StartRowType {
    RootS,
    RootC,
    Count,
}

/// Serde for hex
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(transparent)]
pub struct Hex {
    #[serde(with = "hex::serde")]
    bytes: Vec<u8>,
}

impl From<Vec<u8>> for Hex {
    fn from(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
}

impl Deref for Hex {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.bytes
    }
}

/// MPT branch node
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BranchNode {
    pub(crate) modified_index: usize,
    pub(crate) drifted_index: usize,
    pub(crate) list_rlp_bytes: [Hex; 2],
}

/// MPT extension node
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtensionNode {
    pub(crate) list_rlp_bytes: Hex,
}

/// MPT start node
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StartNode {
    pub disable_preimage_check: bool,
    pub proof_type: MPTProofType,
}

/// MPT extension branch node
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtensionBranchNode {
    pub(crate) is_extension: bool,
    pub(crate) is_placeholder: [bool; 2],
    pub(crate) extension: ExtensionNode,
    pub(crate) branch: BranchNode,
}

/// MPT account node
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountNode {
    pub(crate) address: Hex,
    pub(crate) key: Hex,
    pub(crate) list_rlp_bytes: [Hex; 2],
    pub(crate) value_rlp_bytes: [Hex; 2],
    pub(crate) value_list_rlp_bytes: [Hex; 2],
    pub(crate) drifted_rlp_bytes: Hex,
    pub(crate) wrong_rlp_bytes: Hex,
}

/// MPT storage node
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StorageNode {
    pub(crate) address: Hex,
    pub(crate) key: Hex,
    pub(crate) list_rlp_bytes: [Hex; 2],
    pub(crate) value_rlp_bytes: [Hex; 2],
    pub(crate) drifted_rlp_bytes: Hex,
    pub(crate) wrong_rlp_bytes: Hex,
}

/// MPT node
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Node {
    pub(crate) start: Option<StartNode>,
    pub(crate) extension_branch: Option<ExtensionBranchNode>,
    pub(crate) account: Option<AccountNode>,
    pub(crate) storage: Option<StorageNode>,
    /// MPT node values
    pub values: Vec<Hex>,
    /// MPT keccak data
    pub keccak_data: Vec<Hex>,
}

/// MPT node
#[allow(missing_docs)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Node2 {
    Start {
        node: StartNode,
        values: Vec<Hex>,
    },
    ExtensionBranch {
        node: ExtensionBranchNode,
        values: Vec<Hex>,
        keccak_data: Vec<Hex>,
    },
    Account {
        node: AccountNode,
        values: Vec<Hex>,
        keccak_data: Vec<Hex>,
    },
    Storage {
        node: StorageNode,
        values: Vec<Hex>,
        keccak_data: Vec<Hex>,
    },
}

/// RLP types start
pub const NODE_RLP_TYPES_START: [RlpItemType; StartRowType::Count as usize] =
    [RlpItemType::Hash, RlpItemType::Hash];

/// RLP types branch
pub const NODE_RLP_TYPES_BRANCH: [RlpItemType; ExtensionBranchRowType::Count as usize] = [
    RlpItemType::Node,
    RlpItemType::Node,
    RlpItemType::Node,
    RlpItemType::Node,
    RlpItemType::Node,
    RlpItemType::Node,
    RlpItemType::Node,
    RlpItemType::Node,
    RlpItemType::Node,
    RlpItemType::Node,
    RlpItemType::Node,
    RlpItemType::Node,
    RlpItemType::Node,
    RlpItemType::Node,
    RlpItemType::Node,
    RlpItemType::Node,
    RlpItemType::Node,
    RlpItemType::Key,
    RlpItemType::Node,
    RlpItemType::Nibbles,
    RlpItemType::Node,
];

/// RLP types account
pub const NODE_RLP_TYPES_ACCOUNT: [RlpItemType; AccountRowType::Count as usize] = [
    RlpItemType::Key,
    RlpItemType::Key,
    RlpItemType::Value,
    RlpItemType::Value,
    RlpItemType::Hash,
    RlpItemType::Hash,
    RlpItemType::Value,
    RlpItemType::Value,
    RlpItemType::Hash,
    RlpItemType::Hash,
    RlpItemType::Key,
    RlpItemType::Key,
    RlpItemType::Address,
    RlpItemType::Hash,
];

/// RLP types account
pub const NODE_RLP_TYPES_STORAGE: [RlpItemType; StorageRowType::Count as usize] = [
    RlpItemType::Key,
    RlpItemType::Value,
    RlpItemType::Key,
    RlpItemType::Value,
    RlpItemType::Key,
    RlpItemType::Key,
    RlpItemType::Hash,
    RlpItemType::Hash,
];
