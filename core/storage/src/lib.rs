#![allow(clippy::mutable_key_type)]

pub mod error;
pub mod relational;

pub use relational::RelationalStorage;

use ckb_jsonrpc_types::{Script, TransactionWithStatus};
use ckb_types::core::{BlockNumber, BlockView, HeaderView, TransactionView};
use ckb_types::{bytes::Bytes, packed, H160, H256};
use common::{async_trait, PaginationRequest, PaginationResponse, Range, Result};
use core_rpc_types::indexer::Transaction;
use core_rpc_types::{TransactionWithRichStatus, TxRichStatus};
pub use protocol::db::{DBDriver, DBInfo, SimpleBlock, SimpleTransaction};

#[async_trait]
pub trait Storage {
    /// Append the given block to the database.
    async fn append_block(&self, block: BlockView) -> Result<()>;

    /// Rollback a block by block hash and block number from the database.
    async fn rollback_block(&self, block_number: BlockNumber, block_hash: H256) -> Result<()>;

    /// Get live cells from the database according to the given arguments.
    async fn get_live_cells(
        &self,
        out_point: Option<packed::OutPoint>,
        lock_hashes: Vec<H256>,
        type_hashes: Vec<H256>,
        block_range: Option<Range>,
        pagination: PaginationRequest,
    ) -> Result<PaginationResponse<DetailedCell>>;

    /// Get live cells from the database according to the given arguments.
    async fn get_live_cells_ex(
        &self,
        lock_script: Option<Script>,
        type_script: Option<Script>,
        lock_len_range: Option<Range>,
        type_len_range: Option<Range>,
        block_range: Option<Range>,
        capacity_range: Option<Range>,
        data_len_range: Option<Range>,
        pagination: PaginationRequest,
    ) -> Result<PaginationResponse<DetailedCell>>;

    /// Get live cells from the database according to the given arguments, extended version.
    async fn get_historical_live_cells(
        &self,
        lock_hashes: Vec<H256>,
        type_hashes: Vec<H256>,
        tip_block_number: BlockNumber,
        out_point: Option<packed::OutPoint>,
        pagination: PaginationRequest,
    ) -> Result<PaginationResponse<DetailedCell>>;

    /// Get cells from the database according to the given arguments.
    async fn get_cells(
        &self,
        out_point: Option<packed::OutPoint>,
        lock_hashes: Vec<H256>,
        type_hashes: Vec<H256>,
        block_range: Option<Range>,
        pagination: PaginationRequest,
    ) -> Result<PaginationResponse<DetailedCell>>;

    /// Get transactions from the database according to the given arguments.
    async fn get_transactions(
        &self,
        out_point: Option<packed::OutPoint>,
        lock_hashes: Vec<H256>,
        type_hashes: Vec<H256>,
        block_range: Option<Range>,
        limit_cellbase: bool,
        pagination: PaginationRequest,
    ) -> Result<PaginationResponse<TransactionWrapper>>;

    async fn get_transactions_by_hashes(
        &self,
        tx_hashes: Vec<H256>,
        block_range: Option<Range>,
        pagination: PaginationRequest,
    ) -> Result<PaginationResponse<TransactionWrapper>>;

    async fn get_transactions_by_scripts(
        &self,
        lock_hashes: Vec<H256>,
        type_hashes: Vec<H256>,
        block_range: Option<Range>,
        limit_cellbase: bool,
        pagination: PaginationRequest,
    ) -> Result<PaginationResponse<TransactionWrapper>>;

    /// Get the block from the database.
    /// There are four situations for the combination of `block_hash` and `block_number`:
    /// 1. `block_hash` and `block_number` are both `Some`. Firstly get block by hash and
    /// check the block number is right.
    /// 2. 'block_hash' is `Some` and 'block_number' is 'None'. Get block by block hash.
    /// 3. 'block_hash' is `None` and 'block_number' is 'Some'. Get block by block number.
    /// 4. 'block_hash' and `block_number` are both None. Get tip block.
    async fn get_block(
        &self,
        block_hash: Option<H256>,
        block_number: Option<BlockNumber>,
    ) -> Result<BlockView>;

    /// Get the block header from the database.
    /// There are four situations for the combination of `block_hash` and `block_number`:
    /// 1. `block_hash` and `block_number` are both `Some`. Firstly get block header by hash
    /// and check the block number is right.
    /// 2. 'block_hash' is `Some` and 'block_number' is 'None'. Get block header by block hash.
    /// 3. 'block_hash' is `None` and 'block_number' is 'Some'. Get block header by block number.
    /// 4. 'block_hash' and `block_number` are both None. Get tip block header.
    async fn get_block_header(
        &self,
        block_hash: Option<H256>,
        block_number: Option<BlockNumber>,
    ) -> Result<HeaderView>;

    /// Get scripts from the database according to the given arguments.
    async fn get_scripts(
        &self,
        script_hashes: Vec<H160>,
        code_hash: Vec<H256>,
        args_len: Option<usize>,
        args: Vec<Bytes>,
    ) -> Result<Vec<packed::Script>>;

    /// Get the tip number and block hash in database.
    async fn get_tip(&self) -> Result<Option<(BlockNumber, H256)>>;

    ///
    async fn get_simple_transaction_by_hash(&self, tx_hash: H256) -> Result<SimpleTransaction>;

    ///
    async fn get_spent_transaction_hash(&self, out_point: packed::OutPoint)
        -> Result<Option<H256>>;

    ///
    async fn get_canonical_block_hash(&self, block_number: BlockNumber) -> Result<H256>;

    ///
    async fn get_scripts_by_partial_arg(
        &self,
        code_hash: &H256,
        arg: Bytes,
        offset_location: (u32, u32),
    ) -> Result<Vec<packed::Script>>;

    /// Get lock hash by registered address
    async fn get_registered_address(&self, lock_hash: H160) -> Result<Option<String>>;

    /// Register address
    async fn register_addresses(&self, addresses: Vec<(H160, String)>) -> Result<Vec<H160>>;

    /// Get the database information.
    fn get_db_info(&self) -> Result<DBInfo>;

    /// Get block info
    async fn get_simple_block(
        &self,
        block_hash: Option<H256>,
        block_number: Option<BlockNumber>,
    ) -> Result<SimpleBlock>;

    /// Get the cells for indexer API.
    async fn get_indexer_transactions(
        &self,
        lock_hashes: Option<Script>,
        type_hashes: Option<Script>,
        block_range: Option<Range>,
        pagination: PaginationRequest,
    ) -> Result<PaginationResponse<Transaction>>;

    /// Get the block count.
    async fn indexer_synced_count(&self) -> Result<u64>;

    /// Get the block count.
    async fn block_count(&self) -> Result<u64>;
}

#[derive(Clone, Hash, Debug, PartialEq, Eq)]
pub struct DetailedCell {
    pub epoch_number: u64,
    pub block_number: BlockNumber,
    pub block_hash: H256,
    pub tx_index: u32,
    pub out_point: packed::OutPoint,
    pub cell_output: packed::CellOutput,
    pub cell_data: Bytes,
    pub consumed_block_number: Option<u64>,
    pub consumed_block_hash: Option<H256>,
    pub consumed_tx_hash: Option<H256>,
    pub consumed_tx_index: Option<u32>,
    pub consumed_input_index: Option<u32>,
    pub since: Option<u64>,
}

#[derive(Clone, Hash, Debug)]
pub struct TransactionWrapper {
    pub transaction_with_status: TransactionWithStatus,
    pub transaction_view: TransactionView,
    pub input_cells: Vec<DetailedCell>,
    pub output_cells: Vec<DetailedCell>,
    pub is_cellbase: bool,
    pub timestamp: u64,
}

impl std::convert::From<TransactionWrapper> for TransactionWithRichStatus {
    fn from(tx: TransactionWrapper) -> Self {
        TransactionWithRichStatus {
            transaction: tx.transaction_with_status.transaction,
            tx_status: TxRichStatus {
                status: tx.transaction_with_status.tx_status.status,
                block_hash: tx.transaction_with_status.tx_status.block_hash,
                reason: tx.transaction_with_status.tx_status.reason,
                timestamp: Some(tx.timestamp.into()),
            },
        }
    }
}
