mod operation;
mod query;
mod transfer;
mod utils;

use crate::error::RpcResult;
use crate::types::{
    AdjustAccountPayload, AdvanceQueryPayload, BlockInfo, DepositPayload, GetBalancePayload,
    GetBalanceResponse, GetBlockInfoPayload, GetSpentTransactionPayload,
    GetTransactionInfoResponse, MercuryInfo, QueryResponse, QueryTransactionsPayload,
    SmartTransferPayload, TransactionCompletionResponse, TransactionStatus, TransferPayload,
    TxView, WithdrawPayload,
};
use crate::{CkbRpc, MercuryRpcServer};

use common::anyhow::{anyhow, Result};
use common::{
    hash::blake2b_160, utils::ScriptInfo, Address, AddressPayload, CodeHashIndex, NetworkType,
    PaginationResponse,
};
use core_storage::{DBAdapter, DBInfo, MercuryStore};

use arc_swap::ArcSwap;
use async_trait::async_trait;
use ckb_jsonrpc_types::TransactionView;
use ckb_types::core::{BlockNumber, RationalU256};
use ckb_types::{bytes::Bytes, packed, prelude::*, H160, H256};
use dashmap::DashMap;
use parking_lot::RwLock;

use std::collections::{HashMap, HashSet};
use std::{str::FromStr, thread::ThreadId};

pub const BYTE_SHANNONS: u64 = 100_000_000;
pub const STANDARD_SUDT_CAPACITY: u64 = 142 * BYTE_SHANNONS;
pub const CHEQUE_CELL_CAPACITY: u64 = 162 * BYTE_SHANNONS;
const MIN_CKB_CAPACITY: u64 = 61 * BYTE_SHANNONS;
const INIT_ESTIMATE_FEE: u64 = BYTE_SHANNONS / 1000;
const DEFAULT_FEE_RATE: u64 = 1000;
const MAX_ITEM_NUM: usize = 1000;

lazy_static::lazy_static! {
    pub static ref TX_POOL_CACHE: RwLock<HashSet<packed::OutPoint>> = RwLock::new(HashSet::new());
    pub static ref CURRENT_BLOCK_NUMBER: ArcSwap<BlockNumber> = ArcSwap::from_pointee(0u64);
    pub static ref CURRENT_EPOCH_NUMBER: ArcSwap<RationalU256> = ArcSwap::from_pointee(RationalU256::zero());
    static ref ACP_USED_CACHE: DashMap<ThreadId, Vec<packed::OutPoint>> = DashMap::new();
    static ref SECP256K1_CODE_HASH: ArcSwap<H256> = ArcSwap::from_pointee(H256::default());
    static ref SUDT_CODE_HASH: ArcSwap<H256> = ArcSwap::from_pointee(H256::default());
    static ref ACP_CODE_HASH: ArcSwap<H256> = ArcSwap::from_pointee(H256::default());
    static ref CHEQUE_CODE_HASH: ArcSwap<H256> = ArcSwap::from_pointee(H256::default());
    static ref DAO_CODE_HASH: ArcSwap<H256> = ArcSwap::from_pointee(H256::default());
}

pub struct MercuryRpcImpl<C> {
    storage: MercuryStore<C>,
    builtin_scripts: HashMap<String, ScriptInfo>,
    ckb_client: C,
    network_type: NetworkType,
    cheque_since: RationalU256,
}

#[async_trait]
impl<C: CkbRpc + DBAdapter> MercuryRpcServer for MercuryRpcImpl<C> {
    async fn get_balance(&self, _payload: GetBalancePayload) -> RpcResult<GetBalanceResponse> {
        Ok(GetBalanceResponse {
            balances: vec![],
            block_number: 0,
        })
    }

    async fn get_block_info(&self, _payload: GetBlockInfoPayload) -> RpcResult<BlockInfo> {
        Ok(BlockInfo {
            block_number: 0,
            block_hash: H256::default(),
            parent_hash: H256::default(),
            timestamp: 0,
            transactions: vec![],
        })
    }

    async fn get_transaction_info(&self, _tx_hash: H256) -> RpcResult<GetTransactionInfoResponse> {
        Ok(GetTransactionInfoResponse {
            transaction: None,
            status: TransactionStatus::Committed,
            reason: None,
        })
    }

    async fn query_transactions(
        &self,
        _payload: QueryTransactionsPayload,
    ) -> RpcResult<PaginationResponse<TxView>> {
        Ok(PaginationResponse {
            response: vec![],
            next_cursor: None,
            count: None,
        })
    }

    async fn build_adjust_account_transaction(
        &self,
        _payload: AdjustAccountPayload,
    ) -> RpcResult<Option<TransactionCompletionResponse>> {
        Ok(None)
    }

    async fn build_transfer_transaction(
        &self,
        _payload: TransferPayload,
    ) -> RpcResult<TransactionCompletionResponse> {
        Ok(TransactionCompletionResponse {
            tx_view: TransactionView::default(),
            sig_entries: vec![],
        })
    }

    async fn build_smart_transfer_transaction(
        &self,
        _payload: SmartTransferPayload,
    ) -> RpcResult<TransactionCompletionResponse> {
        Ok(TransactionCompletionResponse {
            tx_view: TransactionView::default(),
            sig_entries: vec![],
        })
    }

    async fn register_addresses(&self, _addresses: Vec<String>) -> RpcResult<Vec<H160>> {
        Ok(vec![])
    }

    fn get_mercury_info(&self) -> RpcResult<MercuryInfo> {
        Ok(MercuryInfo {
            network_type: NetworkType::Testnet,
            mercury_version: Default::default(),
            ckb_node_version: Default::default(),
            enabled_extensions: vec![],
        })
    }

    fn get_db_info(&self) -> RpcResult<DBInfo> {
        Ok(DBInfo::default())
    }

    async fn build_deposit_transaction(
        &self,
        _payload: DepositPayload,
    ) -> RpcResult<TransactionCompletionResponse> {
        Ok(TransactionCompletionResponse {
            tx_view: TransactionView::default(),
            sig_entries: vec![],
        })
    }

    async fn build_withdraw_transaction(
        &self,
        _payload: WithdrawPayload,
    ) -> RpcResult<TransactionCompletionResponse> {
        Ok(TransactionCompletionResponse {
            tx_view: TransactionView::default(),
            sig_entries: vec![],
        })
    }

    async fn get_spent_transaction(
        &self,
        _payload: GetSpentTransactionPayload,
    ) -> RpcResult<TxView> {
        todo!()
    }

    async fn advance_query(
        &self,
        _payload: AdvanceQueryPayload,
    ) -> RpcResult<PaginationResponse<QueryResponse>> {
        Ok(PaginationResponse {
            response: vec![],
            next_cursor: None,
            count: None,
        })
    }
}

impl<C: CkbRpc> MercuryRpcImpl<C> {
    pub fn new(
        storage: MercuryStore<C>,
        builtin_scripts: HashMap<String, ScriptInfo>,
        ckb_client: C,
        network_type: NetworkType,
        cheque_since: RationalU256,
    ) -> Self {
        MercuryRpcImpl {
            storage,
            builtin_scripts,
            ckb_client,
            network_type,
            cheque_since,
        }
    }
}

pub fn address_to_script(payload: &AddressPayload) -> packed::Script {
    payload.into()
}

pub fn parse_normal_address(addr: &str) -> Result<Address> {
    Address::from_str(addr).map_err(|e| anyhow!("{:?}", e))
}

pub fn pubkey_to_secp_address(lock_args: Bytes) -> H160 {
    let pubkey_hash = H160::from_slice(&lock_args[0..20]).unwrap();
    let script = packed::Script::from(&AddressPayload::new_short(
        NetworkType::Testnet,
        CodeHashIndex::Sighash,
        pubkey_hash,
    ));

    H160::from_slice(&blake2b_160(script.as_slice())).unwrap()
}

pub fn minstant_elapsed(start: u64) -> f64 {
    (minstant::now() - start) as f64 * minstant::nanos_per_cycle() / 1000f64
}
