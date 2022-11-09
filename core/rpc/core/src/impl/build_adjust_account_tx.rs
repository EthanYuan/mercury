use crate::r#impl::utils::{address_to_identity, calculate_cell_capacity, map_json_items};
use crate::r#impl::{calculate_tx_size, utils, utils_types::TransferComponents};
use crate::{error::CoreError, InnerResult, MercuryRpcImpl};

use ckb_types::core::{Capacity, TransactionView};
use ckb_types::{bytes::Bytes, packed, prelude::*, H256};
use common::address::{is_acp, is_pw_lock};
use common::hash::blake2b_256_to_160;
use common::lazy::{ACP_CODE_HASH, PW_LOCK_CODE_HASH, SECP256K1_CODE_HASH};
use common::utils::{decode_udt_amount, encode_udt_amount};
use common::{PaginationRequest, ACP, PW_LOCK, SECP256K1, SUDT};
use core_ckb_client::CkbRpc;
use core_rpc_types::consts::{ckb, DEFAULT_FEE_RATE};
use core_rpc_types::{
    AccountType, AdjustAccountPayload, AssetType, GetAccountInfoPayload, GetAccountInfoResponse,
    Item, LockFilter, ScriptGroup, TransactionCompletionResponse,
};
use core_storage::DetailedCell;
use extension_lock::LockScriptHandler;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::convert::TryInto;

impl<C: CkbRpc> MercuryRpcImpl<C> {
    pub(crate) async fn inner_build_adjust_account_transaction(
        &self,
        mut payload: AdjustAccountPayload,
    ) -> InnerResult<Option<TransactionCompletionResponse>> {
        if payload.asset_info.asset_type == AssetType::CKB {
            return Err(CoreError::AdjustAccountWithoutUDTInfo.into());
        }
        utils::dedup_json_items(&mut payload.from);

        let account_number = payload.account_number.map(Into::into).unwrap_or(1) as usize;
        let fee_rate = payload.fee_rate.map(Into::into).unwrap_or(DEFAULT_FEE_RATE);
        let item: Item = payload.item.clone().try_into()?;
        let acp_address = self.get_acp_address_by_item(&item).await?;
        let mut asset_set = HashSet::new();
        asset_set.insert(payload.asset_info.clone());

        let live_acps = self
            .get_live_cells_by_item(
                Item::Address(acp_address.to_string()),
                asset_set,
                None,
                None,
                HashMap::new(),
                None,
                &mut PaginationRequest::default(),
            )
            .await?;
        let live_acps_len = live_acps.len();

        if live_acps_len == account_number {
            return Ok(None);
        }

        if live_acps_len < account_number {
            self.build_transaction_with_adjusted_fee(
                |rpc, payload, fixed_fee| {
                    Self::build_create_acp_transaction_fixed_fee(
                        rpc,
                        account_number - live_acps_len,
                        payload,
                        fixed_fee,
                    )
                },
                payload.clone(),
                payload.fee_rate.map(Into::into),
            )
            .await
            .map(Some)
        } else {
            let res = self
                .build_collect_asset_transaction_fixed_fee(
                    live_acps,
                    live_acps_len - account_number,
                    fee_rate,
                )
                .await?;

            Ok(Some(TransactionCompletionResponse::new(res.0, res.1)))
        }
    }

    async fn build_create_acp_transaction_fixed_fee(
        &self,
        acp_need_count: usize,
        payload: AdjustAccountPayload,
        fixed_fee: u64,
    ) -> InnerResult<(TransactionView, Vec<ScriptGroup>, usize)> {
        let mut transfer_components = TransferComponents::new();

        let item: Item = payload.item.clone().try_into()?;
        let from = map_json_items(payload.from)?;
        let extra_ckb = payload.extra_ckb.map(Into::into).unwrap_or_else(|| ckb(1));
        let lock_script = self.get_acp_lock_by_item(&item).await?;

        transfer_components.script_deps.insert(SUDT.to_string());

        let sudt_type_script = self
            .build_sudt_type_script(blake2b_256_to_160(&payload.asset_info.udt_hash))
            .await?;
        let type_script_opt = Some(sudt_type_script.clone()).pack();
        for _i in 0..acp_need_count {
            let capacity = calculate_cell_capacity(
                &lock_script,
                &type_script_opt,
                Capacity::bytes(Bytes::from(encode_udt_amount(0)).len())
                    .expect("generate capacity"),
            );

            utils::build_cell_for_output(
                capacity + extra_ckb,
                lock_script.clone(),
                Some(sudt_type_script.clone()),
                Some(0),
                &mut transfer_components.outputs,
                &mut transfer_components.outputs_data,
            )?;
        }

        // balance capacity
        let from = if from.is_empty() { vec![item] } else { from };
        self.prebuild_capacity_balance_tx(from, vec![], None, None, fixed_fee, transfer_components)
            .await
    }

    async fn build_collect_asset_transaction_fixed_fee(
        &self,
        mut acp_cells: Vec<DetailedCell>,
        acp_consume_count: usize,
        fee_rate: u64,
    ) -> InnerResult<(ckb_jsonrpc_types::TransactionView, Vec<ScriptGroup>)> {
        if acp_consume_count > acp_cells.len() {
            return Err(CoreError::InvalidAdjustAccountNumber.into());
        }

        let (inputs, output) = if acp_consume_count == acp_cells.len() {
            let inputs = acp_cells;
            let mut output = inputs
                .get(0)
                .cloned()
                .expect("impossible: get acp cell for output failed");

            let lock_script = if self.is_script(&output.cell_output.lock(), ACP)? {
                let args = output.cell_output.lock().args().raw_data()[0..20].to_vec();
                output
                    .cell_output
                    .lock()
                    .as_builder()
                    .code_hash(
                        SECP256K1_CODE_HASH
                            .get()
                            .expect("get secp256k1 code hash")
                            .pack(),
                    )
                    .args(args.pack())
                    .build()
            } else if self.is_script(&output.cell_output.lock(), PW_LOCK)? {
                output.cell_output.lock()
            } else {
                let lock = output.cell_output.lock();
                let lock_code_hash = lock.code_hash();
                LockScriptHandler::get_normal_script(lock).ok_or_else(|| {
                    CoreError::UnsupportLockScript(hex::encode(lock_code_hash.as_slice()))
                })?
            };
            let type_script: Option<packed::Script> = None;
            let cell = output
                .cell_output
                .as_builder()
                .lock(lock_script)
                .type_(type_script.pack())
                .build();
            output.cell_output = cell;
            (inputs, output)
        } else {
            let _ = acp_cells.split_off(acp_consume_count + 1);

            let inputs = acp_cells;
            let output = inputs
                .get(0)
                .cloned()
                .expect("impossible: get acp cell for output failed");

            (inputs, output)
        };

        let mut input_capacity_sum = 0;
        let mut input_udt_sum = 0;

        for cell in inputs.iter() {
            let capacity: u64 = cell.cell_output.capacity().unpack();
            let amount = decode_udt_amount(&cell.cell_data).unwrap_or(0);
            input_capacity_sum += capacity;
            input_udt_sum += amount;
        }

        let output_data = if acp_consume_count == inputs.len() {
            if input_udt_sum != 0 {
                return Err(CoreError::NotZeroInputUDTAmount.into());
            }
            Bytes::new()
        } else {
            Bytes::from(input_udt_sum.to_le_bytes().to_vec())
        };
        let output = output
            .cell_output
            .as_builder()
            .capacity((input_capacity_sum).pack())
            .build();

        let mut script_deps = BTreeSet::new();
        script_deps.insert(SUDT.to_string());
        let lock_code_hash: H256 = inputs[0].cell_output.lock().code_hash().unpack();
        if lock_code_hash == *ACP_CODE_HASH.get().expect("get secp code hash") {
            script_deps.insert(ACP.to_string());
        }
        if lock_code_hash == *PW_LOCK_CODE_HASH.get().expect("get pw lock code hash") {
            script_deps.insert(SECP256K1.to_string());
            script_deps.insert(PW_LOCK.to_string());
        }
        LockScriptHandler::insert_script_deps(&lock_code_hash, &mut script_deps);

        let mut transfer_components = TransferComponents::new();
        transfer_components.inputs = inputs;
        transfer_components.outputs = vec![output];
        transfer_components.outputs_data = vec![output_data.pack()];
        transfer_components.script_deps = script_deps;

        let (tx_view, script_groups) =
            self.complete_prebuild_transaction(transfer_components, None)?;

        let tx_size = calculate_tx_size(&tx_view);
        let actual_fee = fee_rate.saturating_mul(tx_size as u64) / 1000;

        let tx_view = self.update_tx_view_change_cell_by_index(tx_view.into(), 0, 0, actual_fee)?;
        Ok((tx_view, script_groups))
    }

    pub(crate) async fn inner_get_account_info(
        &self,
        payload: GetAccountInfoPayload,
    ) -> InnerResult<GetAccountInfoResponse> {
        let item: Item = payload.item.clone().try_into()?;
        let acp_address = self.get_acp_address_by_item(&item).await?;

        let mut lock_filters = HashMap::new();
        let account_type = if is_acp(&acp_address) {
            lock_filters.insert(
                ACP_CODE_HASH.get().expect("get built-in acp hash code"),
                LockFilter::default(),
            );
            AccountType::Acp
        } else if is_pw_lock(&acp_address) {
            lock_filters.insert(
                PW_LOCK_CODE_HASH
                    .get()
                    .expect("get built-in pw lock hash code"),
                LockFilter::default(),
            );
            AccountType::PwLock
        } else {
            return Err(CoreError::UnsupportAddress.into());
        };

        let identity_item = Item::Identity(address_to_identity(&acp_address)?);
        let mut asset_set = HashSet::new();
        asset_set.insert(payload.asset_info.clone());
        let live_acps = self
            .get_live_cells_by_item(
                identity_item.clone(),
                asset_set,
                None,
                None,
                lock_filters,
                None,
                &mut PaginationRequest::default(),
            )
            .await?;

        Ok(GetAccountInfoResponse {
            account_number: (live_acps.len() as u32).into(),
            account_address: acp_address.to_string(),
            account_type,
        })
    }
}
