use crate::relational::table::{
    BlockTable, BsonBytes, CanonicalChainTable, CellTable, LiveCellTable, RegisteredAddressTable,
    ScriptTable, TransactionTable, UncleRelationshipTable,
};
use crate::relational::RelationalStorage;
use crate::{error::DBError, relational::to_bson_bytes};

use common::{
    utils, utils::to_fixed_array, DetailedCell, PaginationRequest, PaginationResponse, Range,
    Result,
};
use db_protocol::{SimpleBlock, SimpleTransaction};
use db_xsql::page::PageRequest;
use db_xsql::rbatis::crud::CRUDMut;
use db_xsql::rbatis::plugin::page::Page;

use ckb_types::bytes::Bytes;
use ckb_types::core::{
    BlockBuilder, BlockNumber, BlockView, EpochNumberWithFraction, HeaderBuilder, HeaderView,
    TransactionBuilder, TransactionView, UncleBlockView,
};
use ckb_types::{packed, prelude::*, H256};

use std::collections::HashMap;

const HASH256_LEN: usize = 32;

impl RelationalStorage {
    pub(crate) async fn query_tip(&self) -> Result<Option<(BlockNumber, H256)>> {
        let mut conn = self.pool.acquire().await?;
        let w = self
            .pool
            .wrapper()
            .order_by(false, &["block_number"])
            .limit(1);
        let res: Option<CanonicalChainTable> = conn.fetch_by_wrapper(&w).await?;

        Ok(res.map(|t| {
            (
                t.block_number,
                H256::from_slice(&t.block_hash.bytes[0..32]).unwrap(),
            )
        }))
    }

    pub(crate) async fn get_block_by_number(&self, block_number: BlockNumber) -> Result<BlockView> {
        let block = self.query_block_by_number(block_number).await?;
        self.get_block_view(&block).await
    }

    pub(crate) async fn get_block_by_hash(&self, block_hash: H256) -> Result<BlockView> {
        let block_hash = to_bson_bytes(block_hash.as_bytes());
        let block = self.query_block_by_hash(block_hash).await?;
        self.get_block_view(&block).await
    }

    pub(crate) async fn get_tip_block(&self) -> Result<BlockView> {
        let block = self.query_tip_block().await?;
        self.get_block_view(&block).await
    }

    pub(crate) async fn get_tip_block_header(&self) -> Result<HeaderView> {
        let block = self.query_tip_block().await?;
        Ok(build_header_view(&block))
    }

    pub(crate) async fn get_block_header_by_block_hash(
        &self,
        block_hash: H256,
    ) -> Result<HeaderView> {
        let block_hash = to_bson_bytes(block_hash.as_bytes());
        let block = self.query_block_by_hash(block_hash).await?;
        Ok(build_header_view(&block))
    }

    pub(crate) async fn get_block_header_by_block_number(
        &self,
        block_number: BlockNumber,
    ) -> Result<HeaderView> {
        let block = self.query_block_by_number(block_number).await?;
        Ok(build_header_view(&block))
    }

    async fn get_block_view(&self, block: &BlockTable) -> Result<BlockView> {
        let header = build_header_view(&block);
        let uncles = self.get_uncle_block_views(&block).await?;
        let txs = self
            .get_transactions_by_block_hash(&block.block_hash)
            .await?;
        let proposals = build_proposals(block.proposals.bytes.clone());
        Ok(build_block_view(header, uncles, txs, proposals))
    }

    async fn get_uncle_block_views(&self, block: &BlockTable) -> Result<Vec<UncleBlockView>> {
        let uncles = self.query_uncles_by_hash(&block.block_hash).await?;
        let uncles: Vec<UncleBlockView> = uncles
            .into_iter()
            .map(|uncle| build_uncle_block_view(&uncle))
            .collect();
        Ok(uncles)
    }

    async fn get_transactions_by_block_hash(
        &self,
        block_hash: &BsonBytes,
    ) -> Result<Vec<TransactionView>> {
        let txs = self.query_transactions_by_block_hash(block_hash).await?;
        self.get_transaction_views(txs).await
    }

    pub(crate) async fn query_simple_transaction(
        &self,
        tx_hash: H256,
    ) -> Result<SimpleTransaction> {
        let mut conn = self.pool.acquire().await?;
        let w = self
            .pool
            .wrapper()
            .eq("tx_hash", to_bson_bytes(&tx_hash.0))
            .limit(1);
        let res = conn.fetch_by_wrapper::<CellTable>(&w).await?;

        let epoch_number = EpochNumberWithFraction::new(
            res.epoch_number.into(),
            res.epoch_index.into(),
            res.epoch_length.into(),
        )
        .to_rational();
        let block_hash = H256::from_slice(&res.block_hash.bytes[0..32]).unwrap();
        let block_number = res.block_number;
        let tx_index = res.tx_index as u32;

        Ok(SimpleTransaction {
            epoch_number,
            block_number,
            block_hash,
            tx_index,
        })
    }

    pub(crate) async fn quert_spent_tx_hash(
        &self,
        out_point: packed::OutPoint,
    ) -> Result<Option<H256>> {
        let mut conn = self.pool.acquire().await?;
        let tx_hash: H256 = out_point.tx_hash().unpack();
        let output_index: u32 = out_point.index().unpack();
        let w = self
            .pool
            .wrapper()
            .eq("tx_hash", to_bson_bytes(&tx_hash.0))
            .and()
            .eq("output_index", output_index)
            .limit(1);
        let _res = conn.fetch_by_wrapper::<CellTable>(&w).await?;

        // TODO: fix here
        // if res.consumed_tx_hash.bytes.is_empty() {
        //     Ok(None)
        // } else {
        //     Ok(Some(
        //         H256::from_slice(&res.consumed_tx_hash.bytes[0..32]).unwrap(),
        //     ))
        // }

        Ok(None)
    }

    pub(crate) async fn get_transaction_views(
        &self,
        txs: Vec<TransactionTable>,
    ) -> Result<Vec<TransactionView>> {
        let tx_hashes: Vec<BsonBytes> = txs.iter().map(|tx| tx.tx_hash.clone()).collect();
        let output_cells = self.query_txs_output_cells(&tx_hashes).await?;
        let input_cells = self.query_txs_input_cells(&tx_hashes).await?;

        let mut txs_output_cells: HashMap<Vec<u8>, Vec<CellTable>> = tx_hashes
            .iter()
            .map(|tx_hash| (tx_hash.bytes.clone(), vec![]))
            .collect();
        let mut txs_input_cells: HashMap<Vec<u8>, Vec<CellTable>> = tx_hashes
            .iter()
            .map(|tx_hash| (tx_hash.bytes.clone(), vec![]))
            .collect();
        for cell in output_cells {
            if let Some(set) = txs_output_cells.get_mut(&cell.tx_hash.bytes) {
                (*set).push(cell)
            }
        }
        for cell in input_cells {
            if let Some(set) = txs_input_cells.get_mut(&cell.tx_hash.bytes) {
                (*set).push(cell)
            }
        }

        let tx_views = txs
            .into_iter()
            .map(|tx| {
                let witnesses = build_witnesses(tx.witnesses.bytes.clone());
                let header_deps = build_header_deps(tx.header_deps.bytes.clone());
                let cell_deps = build_cell_deps(tx.cell_deps.bytes.clone());
                let mut inputs = build_cell_inputs(txs_input_cells.get(&tx.tx_hash.bytes));
                if inputs.is_empty() && tx.tx_index == 0 {
                    inputs = vec![build_cell_base_input(tx.block_number)]
                };
                let outputs = build_cell_outputs(txs_output_cells.get(&tx.tx_hash.bytes));
                let outputs_data = build_outputs_data(txs_output_cells.get(&tx.tx_hash.bytes));
                build_transaction_view(
                    tx.version as u32,
                    witnesses,
                    inputs,
                    outputs,
                    outputs_data,
                    cell_deps,
                    header_deps,
                )
            })
            .collect();
        Ok(tx_views)
    }

    pub(crate) async fn get_tip_simple_block(&self) -> Result<SimpleBlock> {
        let tip = self.query_tip().await?;
        let (_, block_hash) = match tip {
            Some((block_number, block_hash)) => (block_number, block_hash),
            None => return Err(DBError::CannotFind.into()),
        };
        let block_table = self
            .query_block_by_hash(to_bson_bytes(block_hash.as_bytes()))
            .await?;
        self.get_simple_block(&block_table).await
    }

    pub(crate) async fn get_simple_block_by_block_number(
        &self,
        block_number: BlockNumber,
    ) -> Result<SimpleBlock> {
        let block_table = self.query_block_by_number(block_number).await?;
        self.get_simple_block(&block_table).await
    }

    pub(crate) async fn get_simple_block_by_block_hash(
        &self,
        block_hash: H256,
    ) -> Result<SimpleBlock> {
        let block_table = self
            .query_block_by_hash(to_bson_bytes(block_hash.as_bytes()))
            .await?;
        self.get_simple_block(&block_table).await
    }

    async fn get_simple_block(&self, block_table: &BlockTable) -> Result<SimpleBlock> {
        let txs = self
            .query_transactions_by_block_hash(&block_table.block_hash)
            .await?;
        Ok(SimpleBlock {
            block_number: block_table.block_number,
            block_hash: bson_to_h256(&block_table.block_hash),
            parent_hash: bson_to_h256(&block_table.parent_hash),
            timestamp: block_table.block_timestamp,
            transactions: txs
                .iter()
                .map(|tx| bson_to_h256(&tx.tx_hash))
                .collect::<Vec<H256>>(),
        })
    }

    pub(crate) async fn query_scripts(
        &self,
        script_hashes: Vec<BsonBytes>,
        code_hash: Vec<BsonBytes>,
        args_len: Option<usize>,
        args: Vec<BsonBytes>,
    ) -> Result<Vec<packed::Script>> {
        if script_hashes.is_empty() && code_hash.is_empty() && args_len.is_none() && args.is_empty()
        {
            return Err(DBError::InvalidParameter(
                "no valid parameter to query scripts".to_owned(),
            )
            .into());
        }

        let mut wrapper = self.pool.wrapper();

        if !script_hashes.is_empty() {
            wrapper = wrapper.in_array("script_hash_160", &script_hashes)
        }

        if !code_hash.is_empty() {
            wrapper = wrapper.and().in_array("script_code_hash", &code_hash);
        }

        if !args.is_empty() {
            wrapper = wrapper.and().in_array("script_args", &args);
        }

        if let Some(len) = args_len {
            wrapper = wrapper.and().eq("script_args_len", len);
        }

        let mut conn = self.pool.acquire().await?;
        let scripts: Vec<ScriptTable> = conn.fetch_list_by_wrapper(&wrapper).await?;

        Ok(scripts.into_iter().map(Into::into).collect())
    }

    pub(crate) async fn query_canonical_block_hash(
        &self,
        block_number: BlockNumber,
    ) -> Result<H256> {
        let mut conn = self.pool.acquire().await?;
        let ret = conn
            .fetch_by_column::<CanonicalChainTable, u64>("block_number", &block_number)
            .await?;
        Ok(bson_to_h256(&ret.block_hash))
    }

    async fn query_cell_by_out_point(&self, out_point: packed::OutPoint) -> Result<DetailedCell> {
        let mut conn = self.pool.acquire().await?;
        let tx_hash: H256 = out_point.tx_hash().unpack();
        let output_index: u32 = out_point.index().unpack();
        let w = self
            .pool
            .wrapper()
            .eq("tx_hash", to_bson_bytes(&tx_hash.0))
            .and()
            .eq("output_index", output_index);

        let res = conn.fetch_by_wrapper::<LiveCellTable>(&w).await?;

        Ok(self.build_detailed_cell(&res, res.data.bytes.clone()))
    }

    pub(crate) async fn query_live_cells(
        &self,
        out_point: Option<packed::OutPoint>,
        lock_hashes: Vec<BsonBytes>,
        type_hashes: Vec<BsonBytes>,
        block_number: Option<BlockNumber>,
        block_range: Option<Range>,
        pagination: PaginationRequest,
    ) -> Result<PaginationResponse<DetailedCell>> {
        if lock_hashes.is_empty()
            && type_hashes.is_empty()
            && block_range.is_none()
            && block_number.is_none()
            && out_point.is_none()
        {
            return Err(DBError::InvalidParameter(
                "no valid parameter to query live cells".to_owned(),
            )
            .into());
        }

        if let Some(op) = out_point {
            let res = self.query_cell_by_out_point(op).await?;
            return Ok(PaginationResponse {
                response: vec![res],
                next_cursor: None,
                count: None,
            });
        }

        let mut wrapper = self.pool.wrapper();

        if !lock_hashes.is_empty() {
            wrapper = wrapper.in_array("lock_hash", &lock_hashes);
        }

        if !type_hashes.is_empty() {
            wrapper = wrapper.and().in_array("script_type_hashes", &type_hashes);
        }

        match (block_number, block_range) {
            (Some(num), None) => wrapper = wrapper.and().eq("block_number", num),

            (None, Some(range)) => {
                wrapper = wrapper
                    .and()
                    .between("block_number", range.min(), range.max())
            }

            (Some(num), Some(range)) => {
                if range.is_in(num) {
                    wrapper = wrapper.and().eq("block_number", &num)
                } else {
                    return Err(DBError::InvalidParameter(format!(
                        "block_number {} is not in range {}",
                        num, range
                    ))
                    .into());
                }
            }

            _ => (),
        }

        let mut conn = self.pool.acquire().await?;
        let limit = pagination.limit.unwrap_or(u64::MAX);
        let mut cells: Page<LiveCellTable> = conn
            .fetch_page_by_wrapper(&wrapper, &PageRequest::from(pagination))
            .await?;
        let mut res = Vec::new();
        let mut next_cursor = None;

        if cells.records.len() as u64 > limit {
            next_cursor = Some(cells.records.pop().unwrap().id);
        }

        for r in cells.records.iter() {
            let cell_data = r.data.bytes.clone();
            res.push(self.build_detailed_cell(r, cell_data));
        }

        Ok(to_pagination_response(res, next_cursor, cells.total))
    }

    fn build_detailed_cell(&self, cell_table: &LiveCellTable, data: Vec<u8>) -> DetailedCell {
        let lock_script = packed::ScriptBuilder::default()
            .code_hash(
                to_fixed_array::<HASH256_LEN>(&cell_table.lock_code_hash.bytes[0..32]).pack(),
            )
            .args(cell_table.lock_args.bytes.pack())
            .hash_type(packed::Byte::new(cell_table.lock_script_type))
            .build();
        let type_script = if cell_table.type_hash.bytes.is_empty() {
            None
        } else {
            Some(
                packed::ScriptBuilder::default()
                    .code_hash(
                        to_fixed_array::<HASH256_LEN>(&cell_table.type_code_hash.bytes[0..32])
                            .pack(),
                    )
                    .args(cell_table.type_args.bytes.pack())
                    .hash_type(packed::Byte::new(cell_table.type_script_type))
                    .build(),
            )
        };

        DetailedCell {
            epoch_number: EpochNumberWithFraction::new_unchecked(
                cell_table.epoch_number.into(),
                cell_table.epoch_index.into(),
                cell_table.epoch_length.into(),
            )
            .to_rational()
            .into_u256(),
            block_number: cell_table.block_number as u64,
            block_hash: H256::from_slice(&cell_table.block_hash.bytes[0..32]).unwrap(),
            tx_index: cell_table.tx_index,
            out_point: packed::OutPointBuilder::default()
                .tx_hash(to_fixed_array::<32>(&cell_table.tx_hash.bytes).pack())
                .index((cell_table.output_index as u32).pack())
                .build(),
            cell_output: packed::CellOutputBuilder::default()
                .lock(lock_script)
                .type_(type_script.pack())
                .capacity(cell_table.capacity.pack())
                .build(),
            cell_data: data.into(),
        }
    }

    // TODO: query refactoring
    async fn query_tip_block(&self) -> Result<BlockTable> {
        let wrapper = self
            .pool
            .wrapper()
            .order_by(false, &["block_number"])
            .limit(1);
        let block: Option<BlockTable> = self.pool.fetch_by_wrapper(&wrapper).await?;
        let block = match block {
            Some(block) => block,
            None => return Err(DBError::CannotFind.into()),
        };
        Ok(block)
    }

    async fn query_block_by_hash(&self, block_hash: BsonBytes) -> Result<BlockTable> {
        let block: Option<BlockTable> =
            self.pool.fetch_by_column("block_hash", &block_hash).await?;
        let block = match block {
            Some(block) => block,
            None => return Err(DBError::CannotFind.into()),
        };
        Ok(block)
    }

    pub(crate) async fn query_block_by_number(
        &self,
        block_number: BlockNumber,
    ) -> Result<BlockTable> {
        let block: Option<BlockTable> = self
            .pool
            .fetch_by_column("block_number", &block_number)
            .await?;
        let block = match block {
            Some(block) => block,
            None => return Err(DBError::WrongHeight.into()),
        };
        Ok(block)
    }

    async fn query_uncles_by_hash(&self, block_hash: &BsonBytes) -> Result<Vec<BlockTable>> {
        let uncle_relationship: Option<UncleRelationshipTable> =
            self.pool.fetch_by_column("block_hash", &block_hash).await?;
        let uncle_relationship = match uncle_relationship {
            Some(uncle_relationship) => uncle_relationship,
            None => return Ok(vec![]),
        };

        if uncle_relationship.uncle_hashes.bytes == packed::Byte32Vec::default().as_bytes().to_vec()
        {
            return Ok(vec![]);
        }

        let uncle_hashes: Vec<BsonBytes> =
            packed::Byte32Vec::new_unchecked(Bytes::from(uncle_relationship.uncle_hashes.bytes))
                .into_iter()
                .map(|hash| to_bson_bytes(hash.as_slice()))
                .collect();
        let uncles: Vec<BlockTable> = self
            .pool
            .fetch_list_by_column("block_hash", &uncle_hashes)
            .await?;
        Ok(uncles)
    }

    pub(crate) async fn query_transactions_by_block_hash(
        &self,
        block_hash: &BsonBytes,
    ) -> Result<Vec<TransactionTable>> {
        let w = self
            .pool
            .wrapper()
            .eq("block_hash", block_hash)
            .order_by(true, &["tx_index"]);
        let txs: Vec<TransactionTable> = self.pool.fetch_list_by_wrapper(&w).await?;
        Ok(txs)
    }

    pub(crate) async fn query_transactions(
        &self,
        tx_hashes: Vec<BsonBytes>,
        lock_hashes: Vec<BsonBytes>,
        type_hashes: Vec<BsonBytes>,
        block_range: Option<Range>,
        pagination: PaginationRequest,
    ) -> Result<PaginationResponse<TransactionTable>> {
        if tx_hashes.is_empty()
            && block_range.is_none()
            && lock_hashes.is_empty()
            && type_hashes.is_empty()
        {
            return Err(DBError::InvalidParameter(
                "no valid parameter to query transactions".to_owned(),
            )
            .into());
        }

        let mut wrapper = self.pool.wrapper();

        if !tx_hashes.is_empty() {
            wrapper = wrapper.in_array("tx_hash", &tx_hashes)
        }

        if let Some(range) = block_range {
            wrapper = wrapper.between("block_number", range.from, range.to);
        }

        if !lock_hashes.is_empty() || !type_hashes.is_empty() {
            wrapper = wrapper
                .and()
                .push_sql("tx_hash in (SELECT tx_hash FROM cell WHERE ");
            let mut w_subquery = self.pool.wrapper().in_array("lock_hash", &lock_hashes);
            if !type_hashes.is_empty() {
                w_subquery = w_subquery.or().in_array("type_hash", &type_hashes);
            }
            wrapper = wrapper.push_wrapper(&w_subquery).push_sql(")");
        }

        let mut conn = self.pool.acquire().await?;
        let limit = pagination.limit.unwrap_or(u64::MAX);
        let mut txs: Page<TransactionTable> = conn
            .fetch_page_by_wrapper(&wrapper, &PageRequest::from(pagination))
            .await?;
        let mut next_cursor = None;

        if txs.records.len() as u64 > limit {
            next_cursor = Some(txs.records.pop().unwrap().id);
        }

        Ok(to_pagination_response(txs.records, next_cursor, txs.total))
    }

    async fn query_txs_output_cells(&self, tx_hashes: &[BsonBytes]) -> Result<Vec<CellTable>> {
        let w = self
            .pool
            .wrapper()
            .r#in("tx_hash", &tx_hashes)
            .order_by(true, &["tx_hash", "output_index"]);
        let cells: Vec<CellTable> = self.pool.fetch_list_by_wrapper(&w).await?;

        Ok(cells)
    }

    async fn query_txs_input_cells(&self, tx_hashes: &[BsonBytes]) -> Result<Vec<CellTable>> {
        let w = self
            .pool
            .wrapper()
            .r#in("consumed_tx_hash", &tx_hashes)
            .order_by(true, &["consumed_tx_hash", "input_index"]);
        let cells: Vec<CellTable> = self.pool.fetch_list_by_wrapper(&w).await?;
        Ok(cells)
    }

    pub(crate) async fn query_registered_address(
        &self,
        lock_hash: BsonBytes,
    ) -> Result<Option<RegisteredAddressTable>> {
        let address = self.pool.fetch_by_column("lock_hash", &lock_hash).await?;
        Ok(address)
    }
}

fn build_block_view(
    header: HeaderView,
    uncles: Vec<UncleBlockView>,
    txs: Vec<TransactionView>,
    proposals: packed::ProposalShortIdVec,
) -> BlockView {
    BlockBuilder::default()
        .header(header)
        .uncles(uncles)
        .transactions(txs)
        .proposals(proposals)
        .build()
}

fn build_uncle_block_view(block: &BlockTable) -> UncleBlockView {
    packed::UncleBlockBuilder::default()
        .header(build_header_view(&block).data())
        .proposals(build_proposals(block.proposals.bytes.clone()))
        .build()
        .into_view()
}

fn build_header_view(block: &BlockTable) -> HeaderView {
    HeaderBuilder::default()
        .number(block.block_number.pack())
        .parent_hash(
            packed::Byte32::from_slice(&block.parent_hash.bytes)
                .expect("impossible: fail to pack parent_hash"),
        )
        .compact_target(block.compact_target.pack())
        .nonce(utils::decode_nonce(&block.nonce.bytes).pack())
        .timestamp(block.block_timestamp.pack())
        .version((block.version as u32).pack())
        .epoch(
            EpochNumberWithFraction::new(
                block.epoch_number.into(),
                block.epoch_index as u64,
                block.epoch_length as u64,
            )
            .number()
            .pack(),
        )
        .dao(packed::Byte32::from_slice(&block.dao.bytes).expect("impossible: fail to pack dao"))
        .transactions_root(
            packed::Byte32::from_slice(&block.transactions_root.bytes)
                .expect("impossible: fail to pack transactions_root"),
        )
        .proposals_hash(
            packed::Byte32::from_slice(&block.proposals_hash.bytes)
                .expect("impossible: fail to pack proposals_hash"),
        )
        .uncles_hash(
            packed::Byte32::from_slice(&block.uncles_hash.bytes)
                .expect("impossible: fail to pack uncles_hash"),
        )
        .build()
}

fn build_witnesses(input: Vec<u8>) -> packed::BytesVec {
    packed::BytesVec::new_unchecked(Bytes::from(input))
}

fn build_header_deps(input: Vec<u8>) -> packed::Byte32Vec {
    packed::Byte32Vec::new_unchecked(Bytes::from(input))
}

fn build_cell_deps(input: Vec<u8>) -> packed::CellDepVec {
    packed::CellDepVec::new_unchecked(Bytes::from(input))
}

fn build_proposals(input: Vec<u8>) -> packed::ProposalShortIdVec {
    packed::ProposalShortIdVec::new_unchecked(Bytes::from(input))
}

fn build_cell_base_input(block_number: u64) -> packed::CellInput {
    let out_point = packed::OutPointBuilder::default()
        .tx_hash(packed::Byte32::default())
        .index(u32::MAX.pack())
        .build();
    packed::CellInputBuilder::default()
        .since(block_number.pack())
        .previous_output(out_point)
        .build()
}

fn build_cell_inputs(input_cells: Option<&Vec<CellTable>>) -> Vec<packed::CellInput> {
    let cells = match input_cells {
        Some(cells) => cells,
        None => return vec![],
    };
    cells
        .iter()
        .map(|cell| {
            let out_point = packed::OutPointBuilder::default()
                .tx_hash(
                    packed::Byte32::from_slice(&cell.tx_hash.bytes)
                        .expect("impossible: fail to pack sinc"),
                )
                .index((cell.output_index as u32).pack())
                .build();
            // TODO: fix here
            packed::CellInputBuilder::default()
                .since(0.pack())
                .previous_output(out_point)
                .build()
        })
        .collect()
}

fn build_cell_outputs(cell_lock_types: Option<&Vec<CellTable>>) -> Vec<packed::CellOutput> {
    let cells = match cell_lock_types {
        Some(cells) => cells,
        None => return vec![],
    };
    cells
        .iter()
        .map(|cell| {
            let lock_script: packed::Script = cell.to_lock_script_table().into();
            let type_script_opt = build_script_opt(if cell.has_type_script() {
                Some(cell.to_type_script_table())
            } else {
                None
            });

            packed::CellOutputBuilder::default()
                .capacity(cell.capacity.pack())
                .lock(lock_script)
                .type_(type_script_opt)
                .build()
        })
        .collect()
}

fn build_script_opt(script_opt: Option<ScriptTable>) -> packed::ScriptOpt {
    let script_opt = script_opt.map(|script| script.into());
    packed::ScriptOptBuilder::default().set(script_opt).build()
}

fn build_outputs_data(cells: Option<&Vec<CellTable>>) -> Vec<packed::Bytes> {
    let cells = match cells {
        Some(cells) => cells,
        None => return vec![],
    };
    cells
        .iter()
        .map(|cell| Bytes::from(cell.data.bytes.clone()).pack())
        .collect()
}

fn build_transaction_view(
    version: u32,
    witnesses: packed::BytesVec,
    inputs: Vec<packed::CellInput>,
    outputs: Vec<packed::CellOutput>,
    outputs_data: Vec<packed::Bytes>,
    cell_deps: packed::CellDepVec,
    header_deps: packed::Byte32Vec,
) -> TransactionView {
    TransactionBuilder::default()
        .version(version.pack())
        .witnesses(witnesses)
        .inputs(inputs)
        .outputs(outputs)
        .outputs_data(outputs_data)
        .cell_deps(cell_deps)
        .header_deps(header_deps)
        .build()
}

pub fn to_pagination_response<T>(
    records: Vec<T>,
    next: Option<i64>,
    total: u64,
) -> PaginationResponse<T> {
    PaginationResponse {
        response: records,
        next_cursor: next,
        count: Some(total),
    }
}

pub fn bson_to_h256(input: &BsonBytes) -> H256 {
    H256::from_slice(&input.bytes[0..32]).unwrap()
}