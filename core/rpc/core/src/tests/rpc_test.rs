use super::*;

use crate::r#impl::utils;
use crate::tests::RpcTestEngine;

use ckb_jsonrpc_types::CellOutput;
use ckb_types::core::BlockNumber;
use ckb_types::packed::{Bytes, OutPoint, Script};
use ckb_types::prelude::Pack;
use ckb_types::{h256, H160, H256};
use common::utils::to_fixed_array;
use common::{Address, DetailedCell, NetworkType, Order, PaginationRequest, Range};
use core::convert::From;
use core_rpc_types::lazy::{CURRENT_BLOCK_NUMBER, CURRENT_EPOCH_NUMBER};
use core_rpc_types::{
    decode_record_id, encode_record_id, indexer, AssetInfo, AssetType, Balance, DaoClaimPayload,
    DaoWithdrawPayload, ExtraType, From as From2, GetBalancePayload, GetBlockInfoPayload, Identity,
    IdentityFlag, Item, JsonItem, Mode, Ownership, Record, RecordId, SinceConfig, SinceFlag,
    SinceType, Source, To, ToInfo, TransactionInfo,
};
use serde::Serialize;
use tokio::test;

use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;

const MAINNET_PG_HOST: &str = "8.210.250.164";
// const TESTNET_PG_HOST: &str = "8.210.169.63";
const TESTNET_PG_HOST: &str = "47.243.115.142";
// const TESTNET_PG_HOST: &str = "47.242.31.83";

#[ignore]
#[tokio::test]
async fn test() {
    let engine = RpcTestEngine::new_pg(NetworkType::Mainnet, "127.0.0.1").await;
    let _rpc = engine.rpc(NetworkType::Mainnet);
}

async fn new_rpc(network: NetworkType) -> MercuryRpcImpl<CkbRpcClient> {
    let host = match network {
        NetworkType::Mainnet => MAINNET_PG_HOST,
        NetworkType::Testnet => TESTNET_PG_HOST,
        _ => unreachable!(),
    };
    let engine = RpcTestEngine::new_pg(network, host).await;
    engine.rpc(network)
}

fn new_identity_from_secp_address(address: &str) -> Identity {
    let address = Address::from_str(address).unwrap();
    assert!(address.is_secp256k1());
    let script = address_to_script(address.payload());
    let pub_key_hash = H160::from_slice(&script.args().as_slice()[4..24]).unwrap();
    println!("pubkey {:?}", pub_key_hash.to_string());
    Identity::new(IdentityFlag::Ckb, pub_key_hash)
}

fn new_identity_from_pw_lock_address(address: &str) -> Identity {
    let address = Address::from_str(address).unwrap();
    assert!(address.is_pw_lock());
    let script = address_to_script(address.payload());
    let pub_key_hash = H160::from_slice(&script.args().as_slice()[4..24]).unwrap();
    println!("pubkey {:?}", pub_key_hash.to_string());
    Identity::new(IdentityFlag::Ethereum, pub_key_hash)
}

fn new_outpoint(tx_id: &str, index: u32) -> OutPoint {
    let tx_hash = H256::from_slice(&hex::decode(tx_id).unwrap())
        .unwrap()
        .pack();
    OutPoint::new(tx_hash, index)
}

fn new_record_id(tx_id: &str, index: u32, address: &str) -> RecordId {
    encode_record_id(
        new_outpoint(tx_id, index),
        Ownership::Address(address.to_string()),
    )
}

fn pretty_print<T: ?Sized + Serialize>(value: &T) {
    println!("{}", serde_json::to_string_pretty(value).unwrap());
}

#[test]
async fn test_record_id() {
    let record_id = new_record_id_2(
        "ecfea4bdf6bf8290d8f8186ed9f4da9b0f8fbba217600b47632f5a72ff677d4d",
        0,
        "52cea1b78b0240f21c1c94af84ce73420c2e9632",
    );
    println!("encode {:?}", hex::encode(record_id.to_vec()));
}

#[test]
async fn test_record_id_2() {
    let record_id = new_record_id_2(
        "ecfea4bdf6bf8290d8f8186ed9f4da9b0f8fbba217600b47632f5a72ff677d4d",
        0,
        "b4266453b38fae3503f05f2a6bd17e4967f15625",
    );
    println!("encode {:?}", hex::encode(record_id.to_vec()));
    let (outpoint, add) = decode_record_id(record_id).unwrap();
    println!("tx_id: {}", outpoint.tx_hash().to_string());
    println!("index: {}", outpoint.index());
    println!("lock_hash: {:?}", add);
}

fn new_record_id_2(tx_id: &str, index: u32, lock_hash: &str) -> RecordId {
    encode_record_id(
        new_outpoint(tx_id, index),
        Ownership::LockHash(lock_hash.to_string()),
    )
}

async fn init_tip(rpc: &MercuryRpcImpl<CkbRpcClient>, tip_block_number: Option<BlockNumber>) {
    let tip_block_number = if let Some(tip_block_number) = tip_block_number {
        tip_block_number
    } else {
        let tip = rpc.inner_get_tip(Context::new()).await.unwrap().unwrap();
        tip.block_number.into()
    };
    let tip_epoch_number = rpc
        .get_epoch_by_number(Context::new(), tip_block_number)
        .await
        .unwrap();
    CURRENT_BLOCK_NUMBER.swap(Arc::new(tip_block_number));
    CURRENT_EPOCH_NUMBER.swap(Arc::new(tip_epoch_number));
}

fn print_scripts(rpc: &MercuryRpcImpl<CkbRpcClient>, scripts: Vec<Script>) {
    for script in scripts {
        let address = rpc.script_to_address(&script);
        println!("address: {}", address.to_string());
    }
}

fn print_cells(rpc: &MercuryRpcImpl<CkbRpcClient>, cells: Vec<DetailedCell>) {
    println!("cells: {:?}", cells.len());
    for cell in cells {
        println!("*****************");
        println!("tx_hash: {}", cell.out_point.tx_hash().to_string());
        println!("output_index: {}", cell.out_point.index());
        println!("cell_output: {}", cell.cell_output);
        println!("cell_data: {}", hex::encode(cell.cell_data));
        println!(
            "address: {}",
            rpc.script_to_address(&cell.cell_output.lock()).to_string()
        );
    }
}

fn print_balances(balances: Vec<Balance>) {
    for balance in balances {
        println!("address_or_lock_hash: {:?}", balance.ownership);
        println!("asset_type: {:?}", balance.asset_info.asset_type);
        println!("udt_hash: {:?}", balance.asset_info.udt_hash.to_string());
        println!(
            "free: {}, occupied: {}, freezed: {}, claimable: {}",
            balance.free, balance.occupied, balance.freezed, balance.claimable
        );
        println!(
            "total: {}",
            balance.free.parse::<u128>().unwrap()
                + balance.occupied.parse::<u128>().unwrap()
                + balance.freezed.parse::<u128>().unwrap()
                + balance.claimable.parse::<u128>().unwrap()
        );
    }
}

fn print_block_info(block_info: BlockInfo) {
    println!("block_number: {}", block_info.block_number);
    println!("block_hash: {}", block_info.block_hash);
    print_transaction_infos(block_info.transactions);
}

fn print_transaction_infos(transaction_infos: Vec<TransactionInfo>) {
    for transaction in transaction_infos {
        println!("******************");
        println!("tx_hash: {}", transaction.tx_hash);
        println!("fee: {}", transaction.fee);
        println!("burn: {:?}", transaction.burn);
        print_records(transaction.records);
    }
}

fn print_records(records: Vec<Record>) {
    for record in records {
        println!("#################");
        println!("block_number: {}", record.block_number);
        println!("occupied: {}", record.occupied);
        println!("asset_type: {:?}", record.asset_info.asset_type);
        println!("udt_hash: {}", record.asset_info.udt_hash.to_string());
        println!("address_or_lock_hash: {:?}", record.ownership);
        println!("status: {:?}", record.status);
        println!("extra: {:?}", record.extra);
    }
}

async fn pretty_print_raw_tx(
    net_ty: NetworkType,
    rpc: &MercuryRpcImpl<CkbRpcClient>,
    raw_transaction: TransactionCompletionResponse,
) {
    let json_string = serde_json::to_string_pretty(&raw_transaction).unwrap();
    std::fs::write("tx.json", json_string.clone()).unwrap();

    let inputs = raw_transaction.tx_view.inner.inputs;
    println!("input shows");
    for input in inputs {
        let tx_hash = input.previous_output.tx_hash;
        let index: u32 = input.previous_output.index.into();
        let tx_wrapper = rpc
            .inner_get_transaction_with_status(Context::new(), tx_hash)
            .await
            .unwrap();
        let output_cell = tx_wrapper.transaction_view.output(index as usize).unwrap();
        let data = tx_wrapper
            .transaction_view
            .outputs_data()
            .get(index as usize)
            .unwrap();
        print_cell_output(net_ty, output_cell.into(), data);
    }

    let outputs = raw_transaction.tx_view.inner.outputs;
    let data = raw_transaction.tx_view.inner.outputs_data;
    println!("output shows");
    for index in 0..outputs.len() {
        print_cell_output(net_ty, outputs[index].clone(), data[index].clone().into());
    }
}

fn print_cell_output(net_ty: NetworkType, output_cell: CellOutput, data: Bytes) {
    let payload = AddressPayload::from_script(&output_cell.lock.into());
    let address = Address::new(net_ty, payload, true);
    let ckb_amount = output_cell.capacity.value();
    let udt_amount = decode_udt_amount(&data.as_slice()[4..]).unwrap_or(0);
    println!(
        "address: {:?}, ckb_amount: {}, udt_amount: {}",
        address.to_string(),
        ckb_amount,
        udt_amount
    );
}

#[test]
async fn test_get_tip() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    let tip = rpc.inner_get_tip(Context::new()).await.unwrap().unwrap();
    println!("tip: {:?}", tip);
}

#[test]
async fn test_get_cells() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;

    let script = packed::Script::new_builder()
        .hash_type(ScriptHashType::Type.into())
        .code_hash(
            h256!("0x58c5f491aba6d61678b7cf7edf4910b1f5e00ec0cde2f42e0abb4fd9aff25a63").pack(),
        )
        .args(
            ckb_types::bytes::Bytes::from(
                h160!("0x6ce722487277b00a0852a943ba3fd5ee03ccca06").as_bytes(),
            )
            .pack(),
        )
        .build();

    let search_key = indexer::SearchKey {
        script: script.into(),
        script_type: indexer::ScriptType::Lock,
        filter: None,
    };

    let res = rpc
        .get_cells(search_key, indexer::Order::Desc, 1.into(), None)
        .await
        .unwrap();
    println!("res: {:?}", res);
}

#[test]
async fn test_get_cells_capacity() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;

    let script = packed::Script::new_builder()
        .hash_type(ScriptHashType::Type.into())
        .code_hash(
            h256!("0x58c5f491aba6d61678b7cf7edf4910b1f5e00ec0cde2f42e0abb4fd9aff25a63").pack(),
        )
        .args(
            ckb_types::bytes::Bytes::from(
                h160!("0x6ce722487277b00a0852a943ba3fd5ee03ccca06").as_bytes(),
            )
            .pack(),
        )
        .build();

    let search_key = indexer::SearchKey {
        script: script.into(),
        script_type: indexer::ScriptType::Lock,
        filter: None,
    };

    let res = rpc.get_cells_capacity(search_key).await.unwrap();
    println!("res: {:?}", res);
}

#[test]
async fn test_get_transactions() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;

    let script = packed::Script::new_builder()
        .hash_type(ScriptHashType::Type.into())
        .code_hash(
            h256!("0x58c5f491aba6d61678b7cf7edf4910b1f5e00ec0cde2f42e0abb4fd9aff25a63").pack(),
        )
        .args(
            ckb_types::bytes::Bytes::from(
                h160!("0x6ce722487277b00a0852a943ba3fd5ee03ccca06").as_bytes(),
            )
            .pack(),
        )
        .build();

    let search_key = indexer::SearchKey {
        script: script.into(),
        script_type: indexer::ScriptType::Lock,
        filter: None,
    };

    let res = rpc
        .get_transactions(search_key, indexer::Order::Desc, 3.into(), None)
        .await
        .unwrap();
    println!("res: {:?}", res);
}

#[test]
async fn test_get_live_cells_by_lock_hash() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;

    let script = packed::Script::new_builder()
        .hash_type(ScriptHashType::Type.into())
        .code_hash(
            h256!("0x58c5f491aba6d61678b7cf7edf4910b1f5e00ec0cde2f42e0abb4fd9aff25a63").pack(),
        )
        .args(
            ckb_types::bytes::Bytes::from(
                h160!("0x6ce722487277b00a0852a943ba3fd5ee03ccca06").as_bytes(),
            )
            .pack(),
        )
        .build();

    let res = rpc
        .get_live_cells_by_lock_hash(script.calc_script_hash().unpack(), 1.into(), 3.into(), None)
        .await
        .unwrap();
    println!("res: {:?}", res);
}

#[test]
async fn test_get_capacity_by_lock_hash() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;

    let script = packed::Script::new_builder()
        .hash_type(ScriptHashType::Type.into())
        .code_hash(
            h256!("0x58c5f491aba6d61678b7cf7edf4910b1f5e00ec0cde2f42e0abb4fd9aff25a63").pack(),
        )
        .args(
            ckb_types::bytes::Bytes::from(
                h160!("0x6ce722487277b00a0852a943ba3fd5ee03ccca06").as_bytes(),
            )
            .pack(),
        )
        .build();

    let res = rpc
        .get_capacity_by_lock_hash(script.calc_script_hash().unpack())
        .await
        .unwrap();
    println!("res: {:?}", res);
}

#[test]
async fn test_get_transactions_by_lock_hash() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;

    let script = packed::Script::new_builder()
        .hash_type(ScriptHashType::Type.into())
        .code_hash(
            h256!("0x58c5f491aba6d61678b7cf7edf4910b1f5e00ec0cde2f42e0abb4fd9aff25a63").pack(),
        )
        .args(
            ckb_types::bytes::Bytes::from(
                h160!("0x6ce722487277b00a0852a943ba3fd5ee03ccca06").as_bytes(),
            )
            .pack(),
        )
        .build();

    let res = rpc
        .get_transactions_by_lock_hash(script.calc_script_hash().unpack(), 1.into(), 3.into(), None)
        .await
        .unwrap();
    println!("res: {:?}", res);
}

#[test]
async fn test_get_scripts_by_identity() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    let identity = new_identity_from_secp_address("ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsq06y24q4tc4tfkgze35cc23yprtpzfrzygljdjh9");
    println!("{:?}", identity);
    let scripts = rpc
        .get_scripts_by_identity(Context::new(), identity, None)
        .await
        .unwrap();
    print_scripts(&rpc, scripts);
}

#[test]
async fn test_get_scripts_by_address_acp() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    let address = Address::from_str("ckt1qq6pngwqn6e9vlm92th84rk0l4jp2h8lurchjmnwv8kq3rt5psf4vq06y24q4tc4tfkgze35cc23yprtpzfrzygsptkzn").unwrap();
    let scripts = rpc
        .get_scripts_by_address(Context::new(), &address, None)
        .await
        .unwrap();
    println!("scripts: {:?}", scripts);
    print_scripts(&rpc, scripts);
}

#[test]
async fn test_get_scripts_by_address_cheque() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    let address = Address::from_str("ckt1qpsdtuu7lnjqn3v8ew02xkwwlh4dv5x2z28shkwt8p2nfruccux4kq29yywse6zu05ez3s64xmtdkl6074rac6zh7h2ln2w035d2lnh32ylk5ydmjq5ypwqs4asnr").unwrap();
    let scripts = rpc
        .get_scripts_by_address(Context::new(), &address, None)
        .await
        .unwrap();
    println!("scripts: {:?}", scripts);
    print_scripts(&rpc, scripts);
}

#[test]
async fn test_get_transactions_by_item_cheque_address() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;

    let item = JsonItem::Address("ckt1qpsdtuu7lnjqn3v8ew02xkwwlh4dv5x2z28shkwt8p2nfruccux4kq29yywse6zu05ez3s64xmtdkl6074rac6zh7h2ln2w035d2lnh32ylk5ydmjq5ypwqs4asnr".to_string());
    let item = Item::try_from(item).unwrap();
    let asset_infos = HashSet::new();

    let ret = rpc
        .get_transactions_by_item(
            Context::new(),
            item,
            asset_infos,
            None,
            None,
            PaginationRequest::default(),
        )
        .await;
    println!("ret: {:?}", ret);
}

#[test]
async fn test_get_secp_address_by_item_acp_address() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    let item = JsonItem::Address("ckt1qq6pngwqn6e9vlm92th84rk0l4jp2h8lurchjmnwv8kq3rt5psf4vq06y24q4tc4tfkgze35cc23yprtpzfrzygsptkzn".to_string());
    let item = Item::try_from(item).unwrap();

    let address = rpc.get_secp_address_by_item(item).unwrap();
    println!("{:?}", address.to_string());
    assert_eq!("ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsq06y24q4tc4tfkgze35cc23yprtpzfrzygljdjh9".to_string(), address.to_string())
}

#[test]
async fn test_get_secp_address_by_item_pw_lock_address() {
    let rpc = new_rpc(NetworkType::Testnet).await;

    // 1
    let item = JsonItem::Address("ckt1q3vvtay34wndv9nckl8hah6fzzcltcqwcrx79apwp2a5lkd07fdxxm88yfy8yaaspgy9922rhglatmsren9qvuknrnz".to_string());
    let item = Item::try_from(item).unwrap();

    let address = rpc.get_secp_address_by_item(item);
    assert!(address.is_err());

    // 2
    let item = JsonItem::Address("ckt1qpvvtay34wndv9nckl8hah6fzzcltcqwcrx79apwp2a5lkd07fdxxqdmda0qd9hukl5r92ujp0nzu6cr4azmudgxac2dp".to_string());
    let item = Item::try_from(item).unwrap();

    let address = rpc.get_secp_address_by_item(item);
    assert!(address.is_err());
}

#[test]
async fn test_get_live_cells_by_item_identity() {
    let rpc = new_rpc(NetworkType::Testnet).await;

    let identity = new_identity_from_secp_address("ckt1qyq8jy6e6hu89lzwwgv9qdx6p0kttl4uax9s79m0mr");

    // 3
    let mut page = PaginationRequest::default();
    page.set_limit(Some(3));
    let cells = rpc
        .get_live_cells_by_item(
            Context::new(),
            Item::Identity(identity.clone()),
            HashSet::new(),
            None,
            None,
            Some((**SECP256K1_CODE_HASH.load()).clone()),
            None,
            false,
            &mut page,
        )
        .await
        .unwrap();

    assert_eq!(3, cells.len());
    println!("page after get: {:?}", page);
    print_cells(&rpc, cells.clone());
    println!();

    // first 2
    let mut page = PaginationRequest::default();
    page.set_limit(Some(2));
    let mut cells_1 = rpc
        .get_live_cells_by_item(
            Context::new(),
            Item::Identity(identity.clone()),
            HashSet::new(),
            None,
            None,
            Some((**SECP256K1_CODE_HASH.load()).clone()),
            None,
            false,
            &mut page,
        )
        .await
        .unwrap();

    assert_eq!(2, cells_1.len());
    println!("page after get: {:?}", page);
    print_cells(&rpc, cells_1.clone());
    println!();

    // second 1
    page.set_limit(Some(1));
    let cells_2 = rpc
        .get_live_cells_by_item(
            Context::new(),
            Item::Identity(identity),
            HashSet::new(),
            None,
            None,
            Some((**SECP256K1_CODE_HASH.load()).clone()),
            None,
            false,
            &mut page,
        )
        .await
        .unwrap();

    assert_eq!(1, cells_2.len());
    println!("page after get: {:?}", page);
    print_cells(&rpc, cells_2.clone());
    println!();

    cells_1.extend(cells_2);
    assert_eq!(cells, cells_1)
}

#[test]
async fn test_get_live_cells_by_item_identity_2() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;

    let mut page = PaginationRequest::default();
    page.set_limit(Some(1000));

    let identity = new_identity_from_secp_address("ckt1qyq27z6pccncqlaamnh8ttapwn260egnt67ss2cwvz");
    println!("{:?}", identity);

    let cells = rpc
        .get_live_cells_by_item(
            Context::new(),
            Item::Identity(identity.clone()),
            HashSet::new(),
            None,
            None,
            None,
            None,
            false,
            &mut page,
        )
        .await
        .unwrap();
    print_cells(&rpc, cells);
}

#[test]
async fn test_get_live_cells_by_item_address() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;

    let mut page = PaginationRequest::default();
    page.set_limit(Some(100));

    let cells = rpc
        .get_live_cells_by_item(
            Context::new(),
            Item::Address("ckt1q3vvtay34wndv9nckl8hah6fzzcltcqwcrx79apwp2a5lkd07fdxxm88yfy8yaaspgy9922rhglatmsren9qvuknrnz".to_string()),
            HashSet::new(),
            None,
            None,
            Some((**PW_LOCK_CODE_HASH.load()).clone()),
            None,
            false,
            &mut page,
        )
        .await
        .unwrap();
    let cells = cells
        .into_iter()
        .filter(|cell| {
            if let Some(type_script) = cell.cell_output.type_().to_opt() {
                let type_code_hash: H256 = type_script.code_hash().unpack();
                type_code_hash != **DAO_CODE_HASH.load()
            } else {
                true
            }
        })
        .collect();
    print_cells(&rpc, cells);
}

// #[test]
// async fn test_get_live_cells_by_record_id() {
//     let rpc = new_rpc(NetworkType::Testnet).await;
//     let record_id = new_record_id(
//         "5ffcd5b3cbe73bd0237bd1ba8d6198228cb28c3a9d532967939890172b2d5904",
//         0,
//         "ckt1qyq8jy6e6hu89lzwwgv9qdx6p0kttl4uax9s79m0mr",
//     );
//     let cells = rpc
//         .get_live_cells_by_item(
//             Item::Record(record_id),
//             HashSet::new(),
//             None,
//             None,
//             None,
//             None,
//             false,
//         )
//         .await
//         .unwrap();
//     print_cells(&rpc, cells);
// }

// #[test]
// async fn test_get_live_cells_by_record_id_lock_hash() {
//     let rpc = new_rpc(NetworkType::Testnet).await;
//     // init_tip(&rpc, None).await;
//     let record_id = new_record_id_2(
//         "52b1cf0ad857d53e1a3552944c1acf268f6a6aea8e8fc85fe8febcb8127d56f0",
//         0,
//         "772dcc93612464ae31d0854a022091ea18bfc5ee",
//     );

//     let cells = rpc
//         .get_live_cells_by_item(
//             Item::Record(record_id),
//             HashSet::new(),
//             None,
//             None,
//             None,
//             None,
//             false,
//         )
//         .await
//         .unwrap();
//     print_cells(&rpc, cells);
// }

// #[test]
// async fn test_get_secp_lock_hash_by_item() {}

// #[test]
// async fn test_to_record() {}

// #[test]
// async fn test_generate_ckb_address_or_lock_hash() {}

#[test]
async fn test_get_balance_by_address() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;
    let item = JsonItem::Address("ckt1qypyfy67hjrqmcyzs2cpvdfhd9lx6mgc68aqjx5d7w".to_string());
    let asset_infos = HashSet::new();
    let payload = GetBalancePayload {
        item,
        asset_infos,
        tip_block_number: None,
    };
    let balances = rpc.inner_get_balance(Context::new(), payload).await;
    print_balances(balances.unwrap().balances);
}

#[test]
async fn test_get_balance_by_address_2() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;
    let item = JsonItem::Address("ckt1qyq8jy6e6hu89lzwwgv9qdx6p0kttl4uax9s79m0mr".to_string());
    let asset_infos = HashSet::new();
    let payload = GetBalancePayload {
        item,
        asset_infos,
        tip_block_number: None,
    };
    let balances = rpc.inner_get_balance(Context::new(), payload).await;
    print_balances(balances.unwrap().balances);
}

#[test]
async fn test_get_balance_by_address_3() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;
    let item = JsonItem::Address("ckt1qyq05g42p2h32knvs9nrf3s4zgzxkzyjxygsmfsj8m".to_string());
    let asset_infos = HashSet::new();
    let payload = GetBalancePayload {
        item,
        asset_infos,
        tip_block_number: None,
    };
    let balances = rpc.inner_get_balance(Context::new(), payload).await;
    println!("{:?}", balances);
    print_balances(balances.unwrap().balances);
}

#[test]
async fn test_get_balance_by_address_4() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;
    let item = JsonItem::Address("ckt1qyq27z6pccncqlaamnh8ttapwn260egnt67ss2cwvz".to_string());
    let asset_infos = HashSet::new();
    let payload = GetBalancePayload {
        item,
        asset_infos,
        tip_block_number: None,
    };
    let balances = rpc.inner_get_balance(Context::new(), payload).await;
    println!("{:?}", balances);
    print_balances(balances.unwrap().balances);
}

#[test]
async fn test_get_balance_by_address_withdraw_i() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;
    let item = JsonItem::Address("ckt1qyq2at204g6kdmrfcdc2yxyr9jdlrmg06enq3r9l8f".to_string());
    let asset_infos = HashSet::new();
    let payload = GetBalancePayload {
        item,
        asset_infos,
        tip_block_number: None,
    };
    let balances = rpc.inner_get_balance(Context::new(), payload).await;
    println!("{:?}", balances);
    print_balances(balances.unwrap().balances);
}

#[test]
async fn test_get_balance_by_pw_lock_address() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;
    let item = JsonItem::Address("ckt1q3vvtay34wndv9nckl8hah6fzzcltcqwcrx79apwp2a5lkd07fdxxm88yfy8yaaspgy9922rhglatmsren9qvuknrnz".to_string());
    let asset_infos = HashSet::new();
    let payload = GetBalancePayload {
        item,
        asset_infos,
        tip_block_number: None,
    };
    let balances = rpc.inner_get_balance(Context::new(), payload).await;
    println!("{:?}", balances);
    print_balances(balances.unwrap().balances);
}

#[test]
async fn test_get_balance_by_identity_for_pw_lock() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;

    let identity = new_identity_from_pw_lock_address("ckt1q3vvtay34wndv9nckl8hah6fzzcltcqwcrx79apwp2a5lkd07fdxxm88yfy8yaaspgy9922rhglatmsren9qvuknrnz");
    let item = JsonItem::Identity(hex::encode(identity.0));
    let asset_infos = HashSet::new();
    let payload = GetBalancePayload {
        item,
        asset_infos,
        tip_block_number: None,
    };
    let balances = rpc.inner_get_balance(Context::new(), payload).await;
    print_balances(balances.unwrap().balances);
}

#[test]
async fn test_get_balance_by_record_for_pw_lock() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;

    let record_id = new_record_id(
        "f1fc108ed3449aa4dbfbf80da707e6515af317ad2edda29243a605c27d70c3c6",
        2,
        "ckt1q3vvtay34wndv9nckl8hah6fzzcltcqwcrx79apwp2a5lkd07fdxxm88yfy8yaaspgy9922rhglatmsren9qvuknrnz",
    );
    let record_id = hex::encode(record_id.to_vec());
    println!("{:?}", record_id);
    let item = JsonItem::Record(record_id);
    let asset_infos = HashSet::new();
    let payload = GetBalancePayload {
        item,
        asset_infos,
        tip_block_number: None,
    };
    let balances = rpc.inner_get_balance(Context::new(), payload).await;
    print_balances(balances.unwrap().balances);
}

#[test]
async fn test_get_balance_by_identity() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;

    let identity = new_identity_from_secp_address("ckt1qyq27z6pccncqlaamnh8ttapwn260egnt67ss2cwvz");
    let item = JsonItem::Identity(hex::encode(identity.0));
    let asset_infos = HashSet::new();
    let payload = GetBalancePayload {
        item,
        asset_infos,
        tip_block_number: None,
    };
    let balances = rpc.inner_get_balance(Context::new(), payload).await;
    print_balances(balances.unwrap().balances);
}

#[test]
async fn test_get_balance_by_identity_2() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;

    let identity = new_identity_from_secp_address("ckt1qyq05g42p2h32knvs9nrf3s4zgzxkzyjxygsmfsj8m");
    let item = JsonItem::Identity(hex::encode(identity.0));
    let asset_infos = HashSet::new();
    let payload = GetBalancePayload {
        item,
        asset_infos,
        tip_block_number: None,
    };
    let balances = rpc.inner_get_balance(Context::new(), payload).await;
    print_balances(balances.unwrap().balances);
}

#[test]
async fn test_get_balance_by_identity_3() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;

    let identity = new_identity_from_secp_address("ckt1qyq27z6pccncqlaamnh8ttapwn260egnt67ss2cwvz"); // secp + acp + cheque
    let item = JsonItem::Identity(hex::encode(identity.0));
    let asset_infos = HashSet::new();
    let payload = GetBalancePayload {
        item,
        asset_infos,
        tip_block_number: None,
    };
    let balances = rpc.inner_get_balance(Context::new(), payload).await;
    println!("{:?}", balances);
    print_balances(balances.unwrap().balances);
}

#[test]
async fn test_get_balance_by_record_id() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;

    let item = JsonItem::Record("0xfc43d8bdfff3051f3c908cd137e0766eecba4e88ae5786760c3e0e0f1d76c0040000000200636b74317179716738386363716d35396b7378703835373838706e716734726b656a646763673271786375327166".to_string());
    let asset_infos = HashSet::new();
    let payload = GetBalancePayload {
        item,
        asset_infos,
        tip_block_number: None,
    };
    let balances = rpc.inner_get_balance(Context::new(), payload).await;
    print_balances(balances.unwrap().balances);
}

#[test]
async fn test_get_balance_by_record_id_address() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;

    let record_id = new_record_id(
        "0a7df580b534769fc9933e904300da6aadfa61cebb95805d07ae5bcebefe9c56",
        1,
        "ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsqf3dm7hhmpwrze2semwa5fv2zusw94e9vs4vxmyz",
    );
    let record_id = hex::encode(record_id.to_vec());
    println!("{:?}", record_id);
    let item = JsonItem::Record(record_id);

    let asset_infos = HashSet::new();
    // let assert_info = AssetInfo {
    //     asset_type: AssetType::UDT,
    //     udt_hash: H256::from_str(
    //         "f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd",
    //     )
    //     .unwrap(),
    // };
    // asset_infos.insert(assert_info);

    let payload = GetBalancePayload {
        item,
        asset_infos,
        tip_block_number: Some(3818475),
    };
    let balances = rpc.inner_get_balance(Context::new(), payload).await;
    print_balances(balances.unwrap().balances);
}

// #[test]
// async fn test_get_balance_by_record_id() {
//     let rpc = new_rpc(NetworkType::Testnet).await;
//     init_tip(&rpc, None).await;

//     let record_id = new_record_id_2(
//         "ecfea4bdf6bf8290d8f8186ed9f4da9b0f8fbba217600b47632f5a72ff677d4d",
//         0,
//         "57f5d5f9a9cf8d1aafcef1513f6a11bb902840b8",
//     );
//     let item = JsonItem::Record(hex::encode(record_id.to_vec()));
//     let asset_infos = HashSet::new();
//     let payload = GetBalancePayload {
//         item,
//         asset_infos,
//         tip_block_number: None,
//     };
//     let balances = rpc.inner_get_balance(payload).await;
//     print_balances(balances.unwrap().balances);
// }

// #[test]
// async fn test_get_balance_with_dao_deposit() {
//     let rpc = new_rpc(NetworkType::Mainnet).await;
//     init_tip(&rpc, None).await;

//     let item = JsonItem::Address("ckb1qyq9r0aky9z8qh5t4y665lz2a6djm03kky0s5pp24p".to_string());
//     let asset_infos = HashSet::new();
//     let payload = GetBalancePayload {
//         item,
//         asset_infos,
//         tip_block_number: None,
//     };
//     let balances = rpc.inner_get_balance(payload).await;
//     print_balances(balances.unwrap().balances);
// }

// // select encode(tx_hash, 'hex'), encode(data, 'hex') from mercury_live_cell where type_code_hash = decode('82d76d1b75fe2fd9a27dfbaa65a039221a380d76c926f378d3f81cf3e7e13f2e', 'hex') and data != decode('0000000000000000', 'hex') and lock_code_hash = decode('9bd7e06f3ecf4be0f2fcd2188b23f1b9fcc88e5d4b65a8637b17723bbda3cce8', 'hex');
// #[test]
// async fn test_get_balance_with_dao_withdraw() {
//     let rpc = new_rpc(NetworkType::Mainnet).await;
//     init_tip(&rpc, None).await;

//     let item = JsonItem::Address("ckb1qyqyh3cx7s2q0wpxhjc8a6kdzy8u043sk35s6jvqvv".to_string());
//     let asset_infos = HashSet::new();
//     let payload = GetBalancePayload {
//         item,
//         asset_infos,
//         tip_block_number: None,
//     };
//     let balances = rpc.inner_get_balance(payload).await;
//     print_balances(balances.unwrap().balances);
// }

// #[test]
// async fn test_get_balance_with_cellbase() {
//     let rpc = new_rpc(NetworkType::Mainnet).await;
//     init_tip(&rpc, None).await;

//     let item = JsonItem::Address("ckb1qyqdmeuqrsrnm7e5vnrmruzmsp4m9wacf6vsxasryq".to_string());
//     let asset_infos = HashSet::new();
//     let payload = GetBalancePayload {
//         item,
//         asset_infos,
//         tip_block_number: None,
//     };
//     let balances = rpc.inner_get_balance(payload).await;
//     print_balances(balances.unwrap().balances);
// }

// // 1000000  free: 1787576205864567, occupied: 114850800000000, freezed: 428435038890948, claimable: 0  total: 2330862044755515
// #[test]
// async fn test_get_history_balance_with_cellbase() {
//     let tip_block_number = 1000000;

//     let rpc = new_rpc(NetworkType::Mainnet).await;
//     init_tip(&rpc, Some(tip_block_number)).await;

//     let item = JsonItem::Address("ckb1qyqdmeuqrsrnm7e5vnrmruzmsp4m9wacf6vsxasryq".to_string());
//     let asset_infos = HashSet::new();
//     let payload = GetBalancePayload {
//         item,
//         asset_infos,
//         tip_block_number: Some(tip_block_number),
//     };
//     let balances = rpc.inner_get_balance(payload).await;
//     print_balances(balances.unwrap().balances);
// }

// // 4853527
// #[test]
// async fn test_get_history_balance() {
//     let tip_block_number = 4853527;

//     let rpc = new_rpc(NetworkType::Mainnet).await;
//     init_tip(&rpc, Some(tip_block_number)).await;

//     let item = JsonItem::Address("ckb1qyqqp5ayr86z8txza8n3g2k6ua7430y45qrqgrq786".to_string());
//     let asset_infos = HashSet::new();
//     let payload = GetBalancePayload {
//         item,
//         asset_infos,
//         tip_block_number: Some(tip_block_number),
//     };
//     let balances = rpc.inner_get_balance(payload).await;
//     print_balances(balances.unwrap().balances);
// }

// #[test]
// async fn test_get_epoch_from_number() {
//     let engine = RpcTestEngine::new_pg(NetworkType::Mainnet, MAINNET_PG_HOST).await;
//     let rpc = engine.rpc(NetworkType::Mainnet);

//     let block_number = 5364736;
//     let epoch_number = rpc.get_epoch_by_number(block_number).await;
//     println!("epoch_number: {:?}", epoch_number);
// }

// #[test]
// async fn test_get_block_info_of_tip() {
//     let rpc = new_rpc(NetworkType::Mainnet).await;
//     init_tip(&rpc, None).await;

//     let payload = GetBlockInfoPayload {
//         block_number: None,
//         block_hash: None,
//     };
//     let block_info = rpc.inner_get_block_info(payload).await.unwrap();
//     print_block_info(block_info);
// }

#[test]
async fn test_get_block_info_for_pw_lock() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;

    let payload = GetBlockInfoPayload {
        block_number: Some(3804091),
        block_hash: None,
    };
    let block_info = rpc
        .inner_get_block_info(Context::new(), payload)
        .await
        .unwrap();
    print_block_info(block_info);
}

// #[test]
// async fn test_get_block_info_of_block_hash() {
//     let rpc = new_rpc(NetworkType::Mainnet).await;

//     let payload = GetBlockInfoPayload {
//         block_number: None,
//         block_hash: Some(
//             H256::from_str("9a1f2ebe4644978e003c6c8ed16684426fb67c176a23db696d09d220c1d6eaf8")
//                 .unwrap(),
//         ),
//     };
//     let block_info = rpc.inner_get_block_info(payload).await.unwrap();
//     print_block_info(block_info);
// }

// #[test]
// async fn test_get_transaction_info_of_dao() {
//     let rpc = new_rpc(NetworkType::Mainnet).await;

//     let tx_hash =
//         H256::from_str("4db90d39520c59481c434c83e9f9bd1435f7da8df67015fd8fff2a8b08d14fba").unwrap();
//     let transaction = rpc.inner_get_transaction_info(tx_hash).await.unwrap();
//     print_transaction_infos(vec![transaction.transaction.unwrap()]);
// }

// #[test]
// async fn test_get_transaction_info_of_dao_claim() {
//     let rpc = new_rpc(NetworkType::Mainnet).await;

//     let tx_hash =
//         H256::from_str("3e08cbe01920ffc615a0a7cd89292d2ae1d77fedf23e639a84f44967cdcd1798").unwrap();
//     let transaction = rpc.inner_get_transaction_info(tx_hash).await.unwrap();
//     print_transaction_infos(vec![transaction.transaction.unwrap()]);
// }

// #[test]
// async fn test_get_transaction_info_of_cell_base() {
//     let rpc = new_rpc(NetworkType::Mainnet).await;

//     let tx_hash =
//         H256::from_str("5c1762e5fea2fd59c98dc483aaecac9fc5fdc7402e018aa973437661314aaedb").unwrap();
//     let transaction = rpc.inner_get_transaction_info(tx_hash).await.unwrap();
//     print_transaction_infos(vec![transaction.transaction.unwrap()]);
// }

// #[test]
// async fn test_get_transaction_info_of_udt_mint() {
//     let rpc = new_rpc(NetworkType::Mainnet).await;

//     let tx_hash =
//         H256::from_str("c219650853268e6948e51c053eca2e3f408668aa86b32856c996f2a6653d4dc1").unwrap();
//     let transaction = rpc.inner_get_transaction_info(tx_hash).await.unwrap();
//     print_transaction_infos(vec![transaction.transaction.unwrap()]);
// }

#[test]
async fn test_get_transaction_info_for_pw_lock() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;

    let tx_hash =
        H256::from_str("5dd54476569c25dcb004abe41f9c1b0f8049e666f3f6a749b449d5dc9551504f").unwrap();
    let transaction = rpc
        .inner_get_transaction_info(Context::new(), tx_hash)
        .await
        .unwrap();
    print_transaction_infos(vec![transaction.transaction.unwrap()]);
}

#[test]
async fn test_get_spent_transaction_with_double_entry_for_pw_lock() {
    let rpc = new_rpc(NetworkType::Testnet).await;
    init_tip(&rpc, None).await;

    let payload = GetSpentTransactionPayload {
        outpoint: new_outpoint(
            "635f214cdab44251000c2f1b631869a0660be1873174ed1f4cc9cdfff77fbb43",
            1,
        )
        .into(),
        structure_type: StructureType::DoubleEntry,
    };

    let transaction = rpc
        .inner_get_spent_transaction(Context::new(), payload)
        .await
        .unwrap();
    println!("transaction: {:?}", transaction);
}

#[test]
async fn test_get_spent_transaction_with_native_type() {
    let rpc = new_rpc(NetworkType::Testnet).await;

    let payload = GetSpentTransactionPayload {
        outpoint: new_outpoint(
            "635f214cdab44251000c2f1b631869a0660be1873174ed1f4cc9cdfff77fbb43",
            1,
        )
        .into(),
        structure_type: StructureType::Native,
    };

    let transaction = rpc
        .inner_get_spent_transaction(Context::new(), payload)
        .await
        .unwrap();
    println!("transaction: {:?}", transaction);
}

// #[test]
// async fn test_build_deposit() {
//     let net_ty = NetworkType::Mainnet;
//     let rpc = new_rpc(net_ty).await;

//     let items = vec![JsonItem::Address(
//         "ckb1qyqgf9tl0ecx6an7msqllp0jfe99j64qtwcqhfsug7".to_string(),
//     )];
//     let payload = DepositPayload {
//         from: From2 {
//             items,
//             source: Source::Free,
//         },
//         to: None,
//         amount: 200_00000000,
//         fee_rate: None,
//     };

//     let raw_transaction = rpc.inner_build_deposit_transaction(payload).await.unwrap();
//     pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
// }

#[test]
async fn test_build_dao_deposit() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let items = vec![JsonItem::Address(
        "ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsqfqyerlanzmnkxtmd9ww9n7gr66k8jt4tclm9jnk".to_string(),
    )];
    let payload = DaoDepositPayload {
        from: From2 {
            items,
            source: Source::Free,
        },
        to: None,
        amount: 200_00000000,
        fee_rate: None,
    };

    let raw_transaction = rpc
        .inner_build_dao_deposit_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_dao_deposit_pw_lock() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let items = vec![JsonItem::Address(
        "ckt1qpvvtay34wndv9nckl8hah6fzzcltcqwcrx79apwp2a5lkd07fdxxqdd40lmnsnukjh3qr88hjnfqvc4yg8g0gskp8ffv".to_string(),
    )];
    let payload = DaoDepositPayload {
        from: From2 {
            items,
            source: Source::Free,
        },
        to: None,
        amount: 200_00000000,
        fee_rate: None,
    };

    let raw_transaction = rpc
        .inner_build_dao_deposit_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

// // select encode(tx_hash, 'hex') from mercury_live_cell where type_code_hash = decode('82d76d1b75fe2fd9a27dfbaa65a039221a380d76c926f378d3f81cf3e7e13f2e', 'hex') and data != decode('0000000000000000', 'hex') limit 10;
// #[test]
// async fn test_build_withdraw() {
//     let net_ty = NetworkType::Mainnet;
//     let rpc = new_rpc(net_ty).await;
//     init_tip(&rpc, None).await;

//     let items = JsonItem::Address("ckb1qyqdnwp9xvkukg3jxsh07ww99tlw7m7ttg6qfcatz0".to_string());
//     let pay_fee = "ckb1qyqgf9tl0ecx6an7msqllp0jfe99j64qtwcqhfsug7".to_string();
//     let payload = WithdrawPayload {
//         from: items,
//         pay_fee: Some(pay_fee),
//         fee_rate: None,
//     };

//     let raw_transaction = rpc.inner_build_withdraw_transaction(payload).await.unwrap();
//     pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
// }

#[test]
async fn test_build_dao_withdraw() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let items = JsonItem::Address("ckt1qyqzqfj8lmx9h8vvhk62uut8us844v0yh2hsnqvvgc".to_string());
    let payload = DaoWithdrawPayload {
        from: items,
        pay_fee: None,
        fee_rate: None,
    };

    let raw_transaction = rpc
        .inner_build_dao_withdraw_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_dao_withdraw_record_id() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let record_id_bytes = new_record_id(
        "ffb3bad18ea2b967b1d1a6e542be1c8df797d3a3d3b4ba013ef89473d966c736",
        0,
        "ckt1qyqzqfj8lmx9h8vvhk62uut8us844v0yh2hsnqvvgc",
    );
    let record_id = hex::encode(record_id_bytes.to_vec());
    let item = JsonItem::Record(record_id);
    let payload = DaoWithdrawPayload {
        from: item,
        pay_fee: Some("ckt1qyqzqfj8lmx9h8vvhk62uut8us844v0yh2hsnqvvgc".to_string()),
        fee_rate: None,
    };

    let raw_transaction = rpc
        .inner_build_dao_withdraw_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_dao_withdraw_pw_lock() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let items = JsonItem::Address("ckt1q3vvtay34wndv9nckl8hah6fzzcltcqwcrx79apwp2a5lkd07fdxxm88yfy8yaaspgy9922rhglatmsren9qvuknrnz".to_string());
    let payload = DaoWithdrawPayload {
        from: items,
        pay_fee: None,
        fee_rate: None,
    };

    let raw_transaction = rpc
        .inner_build_dao_withdraw_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

// #[test]
// async fn test_build_transfer_with_ckb_and_hold_by_from() {
//     let net_ty = NetworkType::Mainnet;
//     let rpc = new_rpc(net_ty).await;
//     init_tip(&rpc, None).await;

//     let asset_info = AssetInfo::new_ckb();
//     let items = vec![JsonItem::Address(
//         "ckb1qyqgf9tl0ecx6an7msqllp0jfe99j64qtwcqhfsug7".to_string(),
//     )];
//     let to_info = ToInfo {
//         address: "ckb1qyqdnwp9xvkukg3jxsh07ww99tlw7m7ttg6qfcatz0".to_string(),
//         amount: "96500000000".to_string(),
//     };
//     let payload = TransferPayload {
//         asset_info,
//         from: From2 {
//             items,
//             source: Source::Free,
//         },
//         to: To {
//             to_infos: vec![to_info],
//             mode: Mode::HoldByFrom,
//         },
//         pay_fee: None,
//         change: None,
//         fee_rate: None,
//         since: None,
//     };

//     let raw_transaction = rpc.inner_build_transfer_transaction(payload).await.unwrap();
//     pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
// }

// #[test]
// async fn test_build_transfer_with_ckb_and_hold_by_from_with_since() {
//     let net_ty = NetworkType::Mainnet;
//     let rpc = new_rpc(net_ty).await;
//     init_tip(&rpc, None).await;

//     let asset_info = AssetInfo::new_ckb();
//     let items = vec![JsonItem::Address(
//         "ckb1qyqgf9tl0ecx6an7msqllp0jfe99j64qtwcqhfsug7".to_string(),
//     )];
//     let to_info = ToInfo {
//         address: "ckb1qyqdnwp9xvkukg3jxsh07ww99tlw7m7ttg6qfcatz0".to_string(),
//         amount: "96500000000".to_string(),
//     };
//     let since = SinceConfig {
//         flag: SinceFlag::Absolute,
//         type_: SinceType::BlockNumber,
//         value: 6000000,
//     };
//     let payload = TransferPayload {
//         asset_info,
//         from: From2 {
//             items,
//             source: Source::Free,
//         },
//         to: To {
//             to_infos: vec![to_info],
//             mode: Mode::HoldByFrom,
//         },
//         pay_fee: None,
//         change: None,
//         fee_rate: None,
//         since: Some(since),
//     };

//     let raw_transaction = rpc.inner_build_transfer_transaction(payload).await.unwrap();
//     pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
// }

// #[test]
// async fn test_build_transfer_with_ckb_and_hold_by_from_with_change() {
//     let net_ty = NetworkType::Mainnet;
//     let rpc = new_rpc(net_ty).await;
//     init_tip(&rpc, None).await;

//     let asset_info = AssetInfo::new_ckb();
//     let items = vec![JsonItem::Address(
//         "ckb1qyqgf9tl0ecx6an7msqllp0jfe99j64qtwcqhfsug7".to_string(),
//     )];
//     let to_info = ToInfo {
//         address: "ckb1qyqdnwp9xvkukg3jxsh07ww99tlw7m7ttg6qfcatz0".to_string(),
//         amount: "96500000000".to_string(),
//     };
//     let change = "ckb1qyqqzgqrcs0dfwurn8cwgpdd4e5vke5hrxjq6ns3sq".to_string();
//     let since = SinceConfig {
//         flag: SinceFlag::Absolute,
//         type_: SinceType::BlockNumber,
//         value: 6000000,
//     };
//     let payload = TransferPayload {
//         asset_info,
//         from: From2 {
//             items,
//             source: Source::Free,
//         },
//         to: To {
//             to_infos: vec![to_info],
//             mode: Mode::HoldByFrom,
//         },
//         pay_fee: None,
//         change: Some(change),
//         fee_rate: None,
//         since: Some(since),
//     };

//     let raw_transaction = rpc.inner_build_transfer_transaction(payload).await.unwrap();
//     pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
// }

// #[test]
// async fn test_build_transfer_with_ckb_and_hold_by_from_with_pay_fee() {
//     let net_ty = NetworkType::Mainnet;
//     let rpc = new_rpc(net_ty).await;
//     init_tip(&rpc, None).await;

//     let asset_info = AssetInfo::new_ckb();
//     let items = vec![JsonItem::Address(
//         "ckb1qyqgf9tl0ecx6an7msqllp0jfe99j64qtwcqhfsug7".to_string(),
//     )];
//     let to_info = ToInfo {
//         address: "ckb1qyqdnwp9xvkukg3jxsh07ww99tlw7m7ttg6qfcatz0".to_string(),
//         amount: "96500000000".to_string(),
//     };
//     let change = "ckb1qyqqzgqrcs0dfwurn8cwgpdd4e5vke5hrxjq6ns3sq".to_string();
//     let pay_fee = "ckb1qyqqzgqrcs0dfwurn8cwgpdd4e5vke5hrxjq6ns3sq".to_string();
//     let since = SinceConfig {
//         flag: SinceFlag::Absolute,
//         type_: SinceType::BlockNumber,
//         value: 6000000,
//     };
//     let payload = TransferPayload {
//         asset_info,
//         from: From2 {
//             items,
//             source: Source::Free,
//         },
//         to: To {
//             to_infos: vec![to_info],
//             mode: Mode::HoldByFrom,
//         },
//         pay_fee: Some(pay_fee),
//         change: Some(change),
//         fee_rate: None,
//         since: Some(since),
//     };

//     let raw_transaction = rpc.inner_build_transfer_transaction(payload).await.unwrap();
//     pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
// }

// // ckb acp cell -- tx_hash: d57e1b000b3abaf90a04fdb1be2b2f5e1882b77b77cdf0161553b99e346c4175, index: 0, capacity: 67.99996356
// #[test]
// async fn test_build_transfer_with_ckb_and_hold_by_to_with_pay_fee() {
//     let net_ty = NetworkType::Mainnet;
//     let rpc = new_rpc(net_ty).await;
//     init_tip(&rpc, None).await;

//     let asset_info = AssetInfo::new_ckb();
//     let items = vec![JsonItem::Address(
//         "ckb1qyqgf9tl0ecx6an7msqllp0jfe99j64qtwcqhfsug7".to_string(),
//     )];
//     let to_info = ToInfo {
//         address: "ckb1qypvd79a2xjder5xqx5crvrtq07ca3d55qqs95l0n8".to_string(),
//         amount: "96500000000".to_string(),
//     };
//     let change = "ckb1qyqqzgqrcs0dfwurn8cwgpdd4e5vke5hrxjq6ns3sq".to_string();
//     let pay_fee = "ckb1qyqy5vmywpty6p72wpvm0xqys8pdtxqf6cmsr8p2l0".to_string();
//     let payload = TransferPayload {
//         asset_info,
//         from: From2 {
//             items,
//             source: Source::Free,
//         },
//         to: To {
//             to_infos: vec![to_info],
//             mode: Mode::HoldByTo,
//         },
//         pay_fee: Some(pay_fee),
//         change: Some(change),
//         fee_rate: None,
//         since: None,
//     };

//     let raw_transaction = rpc.inner_build_transfer_transaction(payload).await.unwrap();
//     pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
// }

// #[test]
// async fn test_build_transfer_with_udt_and_hold_by_from() {
//     let net_ty = NetworkType::Testnet;
//     let rpc = new_rpc(net_ty).await;
//     init_tip(&rpc, None).await;

//     let asset_info = AssetInfo::new_udt(
//         H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
//     );
//     let identity = new_identity("ckt1qyq8jy6e6hu89lzwwgv9qdx6p0kttl4uax9s79m0mr");
//     let item = JsonItem::Identity(hex::encode(identity.0));
//     let items = vec![item];
//     let to_info = ToInfo {
//         address: "ckt1qyqv2w7f5kuctnt03kk9l09gwuuy6wpys64s4f8vve".to_string(),
//         amount: "1111".to_string(),
//     };
//     let payload = TransferPayload {
//         asset_info,
//         from: From2 {
//             items,
//             source: Source::Free,
//         },
//         to: To {
//             to_infos: vec![to_info],
//             mode: Mode::HoldByFrom,
//         },
//         pay_fee: None,
//         change: None,
//         fee_rate: None,
//         since: None,
//     };

//     let raw_transaction = rpc.inner_build_transfer_transaction(payload).await.unwrap();
//     pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
// }

// #[test]
// async fn test_build_transfer_with_udt_and_hold_by_from_with_pay_fee() {
//     let net_ty = NetworkType::Testnet;
//     let rpc = new_rpc(net_ty).await;
//     init_tip(&rpc, None).await;

//     let asset_info = AssetInfo::new_udt(
//         H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
//     );
//     let identity = new_identity("ckt1qyq8jy6e6hu89lzwwgv9qdx6p0kttl4uax9s79m0mr");
//     let item = JsonItem::Identity(hex::encode(identity.0));
//     let items = vec![item];
//     let to_info = ToInfo {
//         address: "ckt1qyqv2w7f5kuctnt03kk9l09gwuuy6wpys64s4f8vve".to_string(),
//         amount: "1111".to_string(),
//     };
//     let change = "ckt1qyqv2w7f5kuctnt03kk9l09gwuuy6wpys64s4f8vve".to_string();
//     let pay_fee = "ckt1qyqyfy67hjrqmcyzs2cpvdfhd9lx6mgc68aqukw69v".to_string();
//     let payload = TransferPayload {
//         asset_info,
//         from: From2 {
//             items,
//             source: Source::Free,
//         },
//         to: To {
//             to_infos: vec![to_info],
//             mode: Mode::HoldByFrom,
//         },
//         pay_fee: Some(pay_fee),
//         change: Some(change),
//         fee_rate: None,
//         since: None,
//     };

//     let raw_transaction = rpc.inner_build_transfer_transaction(payload).await.unwrap();
//     pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
// }

#[test]
async fn test_build_transfer_with_udt_and_hold_by_to() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_udt(
        H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
    );
    let identity = new_identity_from_secp_address("ckt1qyq8jy6e6hu89lzwwgv9qdx6p0kttl4uax9s79m0mr");
    let item = JsonItem::Identity(hex::encode(identity.0));
    let items = vec![item];
    let to_info = ToInfo {
        address: "ckt1qypv2w7f5kuctnt03kk9l09gwuuy6wpys64smeamhm".to_string(),
        amount: "1111".to_string(),
    };
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByTo,
        },
        pay_fee: None,
        change: None,
        fee_rate: None,
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_query_transactions_for_miner_address() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let mut asset_infos = HashSet::new();
    asset_infos.insert(AssetInfo::new_ckb());

    let payload = QueryTransactionsPayload {
        item: JsonItem::Address("ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsqw6vjzy9kahx3lyvlgap8dp8ewd8g80pcgcexzrj".to_string()),
        asset_infos,
        extra: None,
        block_range: None,
        pagination: PaginationRequest {
            cursor: Some(ckb_types::bytes::Bytes::from(
                [127, 255, 255, 255, 255, 255, 255, 254].to_vec(),
            )),
            order: Order::Desc,
            limit: Some(50),
            skip: None,
            return_count: false,
        },
        structure_type: StructureType::DoubleEntry,
    };

    let transactions = rpc.inner_query_transactions(Context::new(), payload).await;
    println!("{:?}", transactions);
}

#[test]
async fn test_query_transactions_by_identity() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_infos = HashSet::new();

    let payload = QueryTransactionsPayload {
        item: JsonItem::Identity("0x00fa22aa0aaf155a6c816634c61512046b08923111".to_string()),
        asset_infos,
        extra: None,
        block_range: None,
        pagination: PaginationRequest {
            cursor: Some(ckb_types::bytes::Bytes::from(
                [127, 255, 255, 255, 255, 255, 255, 254].to_vec(),
            )),
            order: Order::Desc,
            limit: Some(10),
            skip: None,
            return_count: true,
        },
        structure_type: StructureType::DoubleEntry,
    };

    let transactions = rpc
        .inner_query_transactions(Context::new(), payload)
        .await
        .unwrap();
    println!("transactions: {:?}", transactions);
    println!("{:?}", transactions.count);
    println!("{:?}", transactions.response);
    let _json_string = serde_json::to_string_pretty(&transactions).unwrap();
}

#[test]
async fn test_query_transactions_by_address_with_pw_lock() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let mut asset_infos = HashSet::new();
    asset_infos.insert(AssetInfo::new_ckb());

    let payload = QueryTransactionsPayload {
        item: JsonItem::Address("ckt1q3vvtay34wndv9nckl8hah6fzzcltcqwcrx79apwp2a5lkd07fdxxm88yfy8yaaspgy9922rhglatmsren9qvuknrnz".to_string()),
        asset_infos,
        extra: None,
        block_range: None,
        pagination: PaginationRequest {
            cursor: Some(ckb_types::bytes::Bytes::from(
                [127, 255, 255, 255, 255, 255, 255, 254].to_vec(),
            )),
            order: Order::Desc,
            limit: Some(1),
            skip: None,
            return_count: false,
        },
        structure_type: StructureType::DoubleEntry,
    };

    let transactions = rpc.inner_query_transactions(Context::new(), payload).await;
    println!("{:?}", transactions);
}

#[test]
async fn test_query_transactions_by_identity_with_pw_lock() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_infos = HashSet::new();

    let payload = QueryTransactionsPayload {
        item: JsonItem::Identity("0x016ce722487277b00a0852a943ba3fd5ee03ccca06".to_string()),
        asset_infos,
        extra: None,
        block_range: None,
        pagination: PaginationRequest {
            cursor: Some(ckb_types::bytes::Bytes::from(
                [127, 255, 255, 255, 255, 255, 255, 254].to_vec(),
            )),
            order: Order::Desc,
            limit: Some(1),
            skip: None,
            return_count: true,
        },
        structure_type: StructureType::DoubleEntry,
    };

    let transactions = rpc
        .inner_query_transactions(Context::new(), payload)
        .await
        .unwrap();
    println!("transactions: {:?}", transactions);
    let _json_string = serde_json::to_string_pretty(&transactions).unwrap();
}

#[test]
async fn test_query_transactions_with_extra() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let mut asset_infos = HashSet::new();
    asset_infos.insert(AssetInfo::new_ckb());

    let payload = QueryTransactionsPayload {
        item: JsonItem::Address("ckt1qyqrc4wkvc95f2wxguxaafwtgavpuqnqkxzqs0375w".to_string()),
        asset_infos,
        extra: Some(ExtraType::Dao),
        block_range: Some(Range::new(0, 30000)),
        pagination: PaginationRequest {
            cursor: Some(ckb_types::bytes::Bytes::from(
                [127, 255, 255, 255, 255, 255, 255, 254].to_vec(),
            )),
            order: Order::Desc,
            limit: Some(50),
            skip: None,
            return_count: true,
        },
        structure_type: StructureType::DoubleEntry,
    };

    let transactions = rpc.inner_query_transactions(Context::new(), payload).await;
    println!("{:?}", transactions);
}

#[test]
async fn test_query_transactions_with_record() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let mut asset_infos = HashSet::new();
    let asset_info = AssetInfo::new_udt(
        H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
    );
    asset_infos.insert(asset_info);

    // decode record_id
    let record_id = "3eb0a1974dd6a2b6c3ba220169cef6eec21e94d2267fab9a4e810accc693c8ed0000000000636b7431717136706e6777716e366539766c6d393274683834726b306c346a703268386c757263686a6d6e7776386b71337274357073663476713036793234713474633474666b677a6533356363323379707274707a66727a79677370746b7a6e".to_string();
    let record_id_bytes = new_record_id(
        "3eb0a1974dd6a2b6c3ba220169cef6eec21e94d2267fab9a4e810accc693c8ed",
        0,
        "ckt1qq6pngwqn6e9vlm92th84rk0l4jp2h8lurchjmnwv8kq3rt5psf4vq06y24q4tc4tfkgze35cc23yprtpzfrzygsptkzn",
    );
    println!("{:?}", hex::encode(record_id_bytes.to_vec()));
    let payload = QueryTransactionsPayload {
        item: JsonItem::Record(record_id),
        asset_infos,
        extra: None,
        block_range: None,
        pagination: PaginationRequest {
            cursor: Some(ckb_types::bytes::Bytes::from(
                [127, 255, 255, 255, 255, 255, 255, 254].to_vec(),
            )),
            order: Order::Desc,
            limit: Some(50),
            skip: None,
            return_count: true,
        },
        structure_type: StructureType::Native,
    };

    let transactions = rpc
        .inner_query_transactions(Context::new(), payload)
        .await
        .unwrap();
    let json_string = serde_json::to_string_pretty(&transactions).unwrap();
    println!("{}", json_string);
}

#[test]
async fn test_query_transactions_first() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;

    let payload = QueryTransactionsPayload {
        item: JsonItem::Address("ckt1qq6pngwqn6e9vlm92th84rk0l4jp2h8lurchjmnwv8kq3rt5psf4vq06y24q4tc4tfkgze35cc23yprtpzfrzygsptkzn".to_string()),
        asset_infos: HashSet::new(),
        extra: None,
        block_range: None,
        pagination: PaginationRequest {
            cursor: Some(ckb_types::bytes::Bytes::from(
                [127, 255, 255, 255, 255, 255, 255, 254].to_vec(),
            )),
            order: Order::Desc,
            limit: Some(1),
            skip: None,
            return_count: false,
        },
        structure_type: StructureType::DoubleEntry,
    };

    let transactions = rpc
        .inner_query_transactions(Context::new(), payload)
        .await
        .unwrap();
    let json_string = serde_json::to_string_pretty(&transactions).unwrap();
    println!("{}", json_string);
}

#[test]
async fn test_query_transactions_second() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;

    let payload = QueryTransactionsPayload {
        item: JsonItem::Address("ckt1qq6pngwqn6e9vlm92th84rk0l4jp2h8lurchjmnwv8kq3rt5psf4vq06y24q4tc4tfkgze35cc23yprtpzfrzygsptkzn".to_string()),
        asset_infos: HashSet::new(),
        extra: None,
        block_range: None,
        pagination: PaginationRequest {
            cursor: Some(ckb_types::bytes::Bytes::from(
                [0, 57, 127, 49, 0, 0, 0, 11].to_vec(),
            )),
            order: Order::Desc,
            limit: Some(2),
            skip: None,
            return_count: false,
        },
        structure_type: StructureType::DoubleEntry,
    };

    let transactions = rpc
        .inner_query_transactions(Context::new(), payload)
        .await
        .unwrap();
    let json_string = serde_json::to_string_pretty(&transactions).unwrap();
    println!("{}", json_string);
}

#[test]
async fn test_query_transactions_third() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;

    let payload = QueryTransactionsPayload {
        item: JsonItem::Address("ckt1qq6pngwqn6e9vlm92th84rk0l4jp2h8lurchjmnwv8kq3rt5psf4vq06y24q4tc4tfkgze35cc23yprtpzfrzygsptkzn".to_string()),
        asset_infos: HashSet::new(),
        extra: None,
        block_range: None,
        pagination: PaginationRequest {
            cursor: Some(ckb_types::bytes::Bytes::from(
                [0, 57, 125, 52, 0, 0, 0, 217].to_vec(),
            )),
            order: Order::Desc,
            limit: Some(2),
            skip: None,
            return_count: false,
        },
        structure_type: StructureType::DoubleEntry,
    };

    let transactions = rpc
        .inner_query_transactions(Context::new(), payload)
        .await
        .unwrap();
    let json_string = serde_json::to_string_pretty(&transactions).unwrap();
    println!("{}", json_string);
}

// #[test]
// async fn test_build_smart_transfer_with_udt() {
//     let net_ty = NetworkType::Testnet;
//     let rpc = new_rpc(net_ty).await;
//     init_tip(&rpc, None).await;

//     let asset_info = AssetInfo::new_udt(
//         H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
//     );
//     let to_info = ToInfo {
//         address: "ckt1qyqg88ccqm59ksxp85788pnqg4rkejdgcg2qxcu2qf".to_string(),
//         amount: "20".to_string(),
//     };
//     let payload = SmartTransferPayload {
//         asset_info,
//         from: vec!["ckt1qyqg88ccqm59ksxp85788pnqg4rkejdgcg2qxcu2qf".to_string()],
//         to: vec![to_info],
//         change: None,
//         fee_rate: Some(1000),
//         since: None,
//     };

//     let raw_transaction = rpc
//         .inner_build_smart_transfer_transaction(payload)
//         .await
//         .unwrap();
//     pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
// }

// #[test]
// async fn test_build_smart_transfer_with_udt_hold_by_to() {
//     let net_ty = NetworkType::Testnet;
//     let rpc = new_rpc(net_ty).await;
//     init_tip(&rpc, None).await;

//     let asset_info = AssetInfo::new_udt(
//         H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
//     );
//     let to_info = ToInfo {
//         address: "ckt1qypg88ccqm59ksxp85788pnqg4rkejdgcg2qggxamt".to_string(),
//         amount: "20".to_string(),
//     };
//     let payload = SmartTransferPayload {
//         asset_info,
//         from: vec!["ckt1qyq27z6pccncqlaamnh8ttapwn260egnt67ss2cwvz".to_string()],
//         to: vec![to_info],
//         change: None,
//         fee_rate: Some(1000),
//         since: None,
//     };

//     let raw_transaction = rpc
//         .inner_build_smart_transfer_transaction(payload)
//         .await
//         .unwrap();
//     pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
// }

#[test]
async fn test_build_transfer() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_udt(
        H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
    );
    let to_info = ToInfo {
        address: "ckt1qyqg88ccqm59ksxp85788pnqg4rkejdgcg2qxcu2qf".to_string(),
        amount: "20".to_string(),
    };
    let payload = SimpleTransferPayload {
        asset_info,
        from: vec![
            "ckt1qyqqtg06h75ymw098r3w0l3u4xklsj04tnsqctqrmc".to_string(),
            "ckt1qyqqtg06h75ymw098r3w0l3u4xklsj04tnsqctqrmc".to_string(),
        ],
        to: vec![to_info],
        change: None,
        fee_rate: Some(1000),
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_simple_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_adjust_account() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_udt(
        H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
    );
    let identity = new_identity_from_secp_address("ckt1qyq8jy6e6hu89lzwwgv9qdx6p0kttl4uax9s79m0mr");
    let item = JsonItem::Identity(hex::encode(identity.0));
    let payload = AdjustAccountPayload {
        item,
        from: HashSet::new(),
        asset_info,
        account_number: None,
        extra_ckb: None,
        fee_rate: None,
    };
    pretty_print(&payload);
    let tx = rpc
        .inner_build_adjust_account_transaction(Context::new(), payload)
        .await
        .unwrap()
        .unwrap();
    pretty_print(&tx);
    pretty_print_raw_tx(net_ty, &rpc, tx).await;
}

#[test]
async fn test_adjust_account_secp_address() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_udt(
        H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
    );

    let payload = AdjustAccountPayload {
        item: JsonItem::Address("ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsq06y24q4tc4tfkgze35cc23yprtpzfrzygljdjh9".to_string()),
        from: HashSet::new(),
        asset_info,
        account_number: Some(1),
        extra_ckb: None,
        fee_rate: None,
    };
    pretty_print(&payload);
    let tx = rpc
        .inner_build_adjust_account_transaction(Context::new(), payload)
        .await
        .unwrap()
        .unwrap();
    pretty_print(&tx);
    pretty_print_raw_tx(net_ty, &rpc, tx).await;
}

#[test]
async fn test_adjust_account_with_pw_lock_address() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    // udt
    let code_hash =
        H256::from_str("c5e5dcf215925f7ef4dfaf5f4b4f105bc321c02776d6e7d52a1db3fcd9d011a4").unwrap();
    let args =
        hex::decode("c43009f083e70ae3fee342d59b8df9eec24d669c1c3a3151706d305f5362c37e").unwrap();
    let script = packed::ScriptBuilder::default()
        .hash_type(ckb_types::core::ScriptHashType::Type.into())
        .code_hash(code_hash.pack())
        .args(ckb_types::bytes::Bytes::from(args).pack())
        .build();
    let udt_script_hash = script.calc_script_hash();
    let udt_script_hash = hex::encode(udt_script_hash.raw_data());
    assert_eq!(
        "526bf9af46da513e9b7021e306790f7f03ae320801f3dc0750b2156fcd3d656c".to_string(),
        udt_script_hash
    );

    let asset_info = AssetInfo::new_udt(
        H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
    );

    // let identity = new_identity("ckt1q3vvtay34wndv9nckl8hah6fzzcltcqwcrx79apwp2a5lkd07fdxxm88yfy8yaaspgy9922rhglatmsren9qvuknrnz");
    // let item = JsonItem::Identity(hex::encode(identity.0));

    let from_identity =
        new_identity_from_secp_address("ckt1qyq8jy6e6hu89lzwwgv9qdx6p0kttl4uax9s79m0mr");
    let from_item = JsonItem::Identity(hex::encode(from_identity.0));
    let mut from = HashSet::new();
    from.insert(from_item);

    let item  = JsonItem::Address("ckt1q3vvtay34wndv9nckl8hah6fzzcltcqwcrx79apwp2a5lkd07fdxxm88yfy8yaaspgy9922rhglatmsren9qvuknrnz".to_string());
    let payload = AdjustAccountPayload {
        item,
        from,
        asset_info,
        account_number: Some(2),
        extra_ckb: None,
        fee_rate: None,
    };
    pretty_print(&payload);
    let tx = rpc
        .inner_build_adjust_account_transaction(Context::new(), payload)
        .await
        .unwrap()
        .unwrap();
    pretty_print(&tx);
    pretty_print_raw_tx(net_ty, &rpc, tx).await;
}

#[test]
async fn test_adjust_account_with_pw_lock_address_with_privkey() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_udt(
        H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
    );

    let from = HashSet::new();
    // let from_item = JsonItem::Address("ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsqthh5pum5pzqpssk47zk67hnd6lm28rnqs4cnj0w".to_string());
    // from.insert(from_item);

    let item  = JsonItem::Address("ckt1qpvvtay34wndv9nckl8hah6fzzcltcqwcrx79apwp2a5lkd07fdxxqdd40lmnsnukjh3qr88hjnfqvc4yg8g0gskp8ffv".to_string());
    let payload = AdjustAccountPayload {
        item,
        from,
        asset_info,
        account_number: Some(8),
        extra_ckb: None,
        fee_rate: None,
    };
    pretty_print(&payload);
    let tx = rpc
        .inner_build_adjust_account_transaction(Context::new(), payload)
        .await
        .unwrap()
        .unwrap();
    pretty_print(&tx);
    pretty_print_raw_tx(net_ty, &rpc, tx).await;
}

#[test]
async fn test_build_dao_claim_transaction() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let identity = new_identity_from_secp_address("ckt1qyqzqfj8lmx9h8vvhk62uut8us844v0yh2hsnqvvgc");
    let item = JsonItem::Identity(hex::encode(identity.0));

    let payload = DaoClaimPayload {
        from: item,
        to: None,
        fee_rate: Some(1000),
    };
    pretty_print(&payload);
    let tx = rpc
        .inner_build_dao_claim_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print(&tx);
    pretty_print_raw_tx(net_ty, &rpc, tx).await;
}

#[test]
async fn test_build_dao_claim_transaction_with_address() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let item = JsonItem::Address("ckt1qyqq4x8yqvfggzazwn49t8h2mv4y8py5ppxs04kctp".to_string());

    let payload = DaoClaimPayload {
        from: item,
        to: None,
        fee_rate: Some(1000),
    };
    pretty_print(&payload);
    let tx = rpc
        .inner_build_dao_claim_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print(&tx);
    pretty_print_raw_tx(net_ty, &rpc, tx).await;
}

#[test]
async fn test_build_dao_claim_transaction_with_pw_lock_address() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let item = JsonItem::Address("ckt1q3vvtay34wndv9nckl8hah6fzzcltcqwcrx79apwp2a5lkd07fdxxm88yfy8yaaspgy9922rhglatmsren9qvuknrnz".to_string());

    let payload = DaoClaimPayload {
        from: item,
        to: None,
        fee_rate: Some(1000),
    };
    pretty_print(&payload);
    let tx = rpc
        .inner_build_dao_claim_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print(&tx);
    pretty_print_raw_tx(net_ty, &rpc, tx).await;
}

#[test]
async fn test_build_ckb_secp_transfer_transaction_with_claim_dao_cell() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_ckb();
    let items = vec![JsonItem::Address(
        "ckt1qyqq4x8yqvfggzazwn49t8h2mv4y8py5ppxs04kctp".to_string(),
    )];
    let to_info = ToInfo {
        address: "ckt1qyqg88ccqm59ksxp85788pnqg4rkejdgcg2qxcu2qf".to_string(),
        amount: "10000000000".to_string(),
    };
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByFrom,
        },
        pay_fee: None,
        change: None,
        fee_rate: Some(1000),
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_record_id() {
    let code_hash =
        H256::from_str("9bd7e06f3ecf4be0f2fcd2188b23f1b9fcc88e5d4b65a8637b17723bbda3cce8").unwrap();
    let args = hex::decode("05a1fabfa84db9e538e2e7fe3ca9adf849f55ce0").unwrap();

    let script = packed::ScriptBuilder::default()
        .hash_type(ckb_types::core::ScriptHashType::Type.into())
        .code_hash(code_hash.pack())
        .args(ckb_types::bytes::Bytes::from(args).pack())
        .build();

    let lock_hash = script.calc_script_hash();
    let lock_hash = hex::encode(lock_hash.raw_data());
    println!("lock_hash: {:?}", lock_hash);
    let record_id = new_record_id_2(
        "ecfea4bdf6bf8290d8f8186ed9f4da9b0f8fbba217600b47632f5a72ff677d4d",
        0,
        &lock_hash,
    );
    let record_id_string = hex::encode(record_id.to_vec());
    println!("{:?}", record_id_string);
    assert_eq!("ecfea4bdf6bf8290d8f8186ed9f4da9b0f8fbba217600b47632f5a72ff677d4d000000000135326365613162373862303234306632316331633934616638346365373334323063326539363332623766663163313436376339316537303532656663306335".to_string(),
    record_id_string);

    // decode record_id
    let (outpoint, ownership) = decode_record_id(record_id).unwrap();
    assert_eq!(
        "ecfea4bdf6bf8290d8f8186ed9f4da9b0f8fbba217600b47632f5a72ff677d4d".to_string(),
        hex::encode(outpoint.tx_hash().raw_data())
    );
    let index: u32 = outpoint.index().unpack();
    assert_eq!(0u32, index);
    assert_eq!(
        "52cea1b78b0240f21c1c94af84ce73420c2e9632b7ff1c1467c91e7052efc0c5".to_string(),
        ownership.to_string()
    );
}

#[test]
async fn test_build_record_id_with_address() {
    let record_id = new_record_id(
        "4329e4c751c95384a51072d4cbc9911a101fd08fc32c687353d016bf38b8b22c",
        0,
        "ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsqvrnuvqd6zmgrqn60rnsesy23mvex5vy9q0g8hfd",
    );
    let record_id_string = hex::encode(record_id.to_vec());
    println!("record_id_string: {:?}", record_id_string);
    assert_eq!("4329e4c751c95384a51072d4cbc9911a101fd08fc32c687353d016bf38b8b22c0000000000636b7431717a646130637230386d38356863386a6c6e6670337a65723778756c656a79777434396b74327272307674687977616135307877737176726e75767164367a6d6772716e3630726e7365737932336d7665783576793971306738686664".to_string(),
    record_id_string);
}

#[test]
async fn test_build_ckb_secp_transfer_transaction_with_fee_rate() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_ckb();
    let items = vec![JsonItem::Address(
        "ckt1qyqqtg06h75ymw098r3w0l3u4xklsj04tnsqctqrmc".to_string(),
    )];
    let to_info = ToInfo {
        address: "ckt1qyqg88ccqm59ksxp85788pnqg4rkejdgcg2qxcu2qf".to_string(),
        amount: "96500000000".to_string(),
    };
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByFrom,
        },
        pay_fee: Some("ckt1qyqqtg06h75ymw098r3w0l3u4xklsj04tnsqctqrmc".to_string()),
        change: None,
        fee_rate: Some(1000000),
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_ckb_secp_transfer_transaction_pw_lock() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_ckb();
    let items = vec![JsonItem::Address(
        "ckt1qpvvtay34wndv9nckl8hah6fzzcltcqwcrx79apwp2a5lkd07fdxxqdd40lmnsnukjh3qr88hjnfqvc4yg8g0gskp8ffv".to_string(),
    )];
    let to_info = ToInfo {
        address: "ckt1qyqg88ccqm59ksxp85788pnqg4rkejdgcg2qxcu2qf".to_string(),
        amount: "10000000000".to_string(),
    };
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByFrom,
        },
        pay_fee: None,
        change: None,
        fee_rate: None,
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_ckb_secp_transfer_transaction_with_change_with_fee_rate() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_ckb();
    let items = vec![JsonItem::Address(
        "ckt1qyqqtg06h75ymw098r3w0l3u4xklsj04tnsqctqrmc".to_string(),
    )];
    let to_info = ToInfo {
        address: "ckt1qyqg88ccqm59ksxp85788pnqg4rkejdgcg2qxcu2qf".to_string(),
        amount: "96500000000".to_string(),
    };
    let change = "ckt1qyqv2w7f5kuctnt03kk9l09gwuuy6wpys64s4f8vve".to_string();
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByFrom,
        },
        pay_fee: Some("ckt1qyqqtg06h75ymw098r3w0l3u4xklsj04tnsqctqrmc".to_string()),
        change: Some(change),
        fee_rate: Some(1000),
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_ckb_secp_transfer_transaction_with_fee_rate_identity() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_ckb();

    let item = JsonItem::Identity("0x00a3b8598e1d53e6c5e89e8acb6b4c34d3adb13f2b".to_string());
    let items = vec![item];

    let to_info = ToInfo {
        address: "ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsqg3ck4edchwv40gz7xsv0vygt9lc3jw04q4vqvcz".to_string(),
        amount: "15000000000".to_string(),
    };
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByFrom,
        },
        pay_fee: Some("ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsqd0pdquvfuq077aemn447shf4d8u5f4a0glzz2g4".to_string()),
        change: None,
        fee_rate: Some(1000),
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_ckb_secp_transfer_transaction_with_sudt_cell() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_ckb();

    let identity = new_identity_from_secp_address("ckt1qyq28wze3cw48ek9az0g4jmtfs6d8td38u4s6hp2s0");
    let item = JsonItem::Identity(hex::encode(identity.0));
    let items = vec![item];
    let to_info = ToInfo {
        address: "ckt1qyqv2w7f5kuctnt03kk9l09gwuuy6wpys64s4f8vve".to_string(),
        amount: "22517556405762".to_string(),
    };
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByFrom,
        },
        pay_fee: None,
        change: None,
        fee_rate: None,
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_ckb_acp_transfer() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_ckb();
    let items = vec![
        JsonItem::Address("ckt1qyqqtg06h75ymw098r3w0l3u4xklsj04tnsqctqrmc".to_string()),
        JsonItem::Address("ckt1qyqqtg06h75ymw098r3w0l3u4xklsj04tnsqctqrmc".to_string()),
    ];
    let to_info = ToInfo {
        address: "ckt1qyqg88ccqm59ksxp85788pnqg4rkejdgcg2qxcu2qf".to_string(),
        amount: "96500000000".to_string(),
    };
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByTo,
        },
        pay_fee: None,
        change: None,
        fee_rate: None,
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_ckb_acp_transfer_transaction_with_pay_fee_with_change() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_ckb();
    let identity = new_identity_from_secp_address("ckt1qyq8jy6e6hu89lzwwgv9qdx6p0kttl4uax9s79m0mr");
    let item = JsonItem::Identity(hex::encode(identity.0));
    let items = vec![item];
    let to_info = ToInfo {
        address: "ckt1qypv2w7f5kuctnt03kk9l09gwuuy6wpys64smeamhm".to_string(),
        amount: "1111".to_string(),
    };
    let change = "ckt1qyqv2w7f5kuctnt03kk9l09gwuuy6wpys64s4f8vve".to_string();
    let pay_fee = "ckt1qyqyfy67hjrqmcyzs2cpvdfhd9lx6mgc68aqukw69v".to_string();
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByTo,
        },
        pay_fee: Some(pay_fee),
        change: Some(change),
        fee_rate: None,
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_ckb_acp_transfer_all_transaction_with_change() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_ckb();
    // let identity = new_identity_from_secp_address("ckt1qyq8jy6e6hu89lzwwgv9qdx6p0kttl4uax9s79m0mr");
    // let item = JsonItem::Identity(hex::encode(identity.0));
    let items = vec![JsonItem::Address(
        "ckt1qyqvuatnjw0xm7ug9sxyddpcsqjemz3zcrrqxtgujx".to_string(),
    )];
    let to_info = ToInfo {
        address: "ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsqgjzsqm383pgjchu8cdxg5kxnr8tdqwewsms9txh".to_string(),
        amount: "25241498936118".to_string(),
    };
    let change = "ckt1qypv2w7f5kuctnt03kk9l09gwuuy6wpys64smeamhm".to_string();
    let pay_fee = "ckt1qyqyfy67hjrqmcyzs2cpvdfhd9lx6mgc68aqukw69v".to_string();
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByFrom,
        },
        pay_fee: Some(pay_fee),
        change: Some(change),
        fee_rate: None,
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_ckb_acp_transfer_transaction_to_pw_lock() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_ckb();
    // let identity = new_identity_from_secp_address("ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsq0jk8hhyea6hnzmtgkct79y0gfpa8lwueqrn57ur");
    let item = JsonItem::Address("ckt1qyqdfwf8fa206yk5lsaazn3w3jfjfuhqhe8qytqyms".to_string());
    let items = vec![item];
    let to_info = ToInfo {
        address: "ckt1qpvvtay34wndv9nckl8hah6fzzcltcqwcrx79apwp2a5lkd07fdxxqdd40lmnsnukjh3qr88hjnfqvc4yg8g0gskp8ffv".to_string(),
        amount: "100000000".to_string(),
    };
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByTo,
        },
        pay_fee: None,
        change: None,
        fee_rate: None,
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_ckb_secp_transfer_transaction_from_pw_lock() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_ckb();
    let item = JsonItem::Address("ckt1qpvvtay34wndv9nckl8hah6fzzcltcqwcrx79apwp2a5lkd07fdxxqdd40lmnsnukjh3qr88hjnfqvc4yg8g0gskp8ffv".to_string());
    let items = vec![item];
    let to_info = ToInfo {
        address: "ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsqthh5pum5pzqpssk47zk67hnd6lm28rnqs4cnj0w".to_string(),
        amount: "6100000000".to_string(),
    };
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByFrom,
        },
        pay_fee: None,
        change: None,
        fee_rate: None,
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_udt_cheque_transfer_transaction() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_udt(
        H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
    );
    let identity = new_identity_from_secp_address("ckt1qyq8jy6e6hu89lzwwgv9qdx6p0kttl4uax9s79m0mr");
    let item = JsonItem::Identity(hex::encode(identity.0));
    let items = vec![item];
    let to_info = ToInfo {
        address: "ckt1qyqv2w7f5kuctnt03kk9l09gwuuy6wpys64s4f8vve".to_string(),
        amount: "1111".to_string(),
    };
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByFrom,
        },
        pay_fee: None,
        change: None,
        fee_rate: None,
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_udt_cheque_transfer_transaction_from_pw_lock() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_udt(
        H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
    );
    let identity = new_identity_from_pw_lock_address("ckt1qpvvtay34wndv9nckl8hah6fzzcltcqwcrx79apwp2a5lkd07fdxxqdd40lmnsnukjh3qr88hjnfqvc4yg8g0gskp8ffv");
    let item = JsonItem::Identity(hex::encode(identity.0));
    let items = vec![item];
    let to_info = ToInfo {
        address: "ckt1qyqv2w7f5kuctnt03kk9l09gwuuy6wpys64s4f8vve".to_string(),
        amount: "1111".to_string(),
    };
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByFrom,
        },
        pay_fee: None,
        change: None,
        fee_rate: None,
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_udt_cheque_transfer_transaction_from_contain_to() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_udt(
        H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
    );
    let identity = new_identity_from_secp_address("ckt1qyq8jy6e6hu89lzwwgv9qdx6p0kttl4uax9s79m0mr");
    let item = JsonItem::Identity(hex::encode(identity.0));
    let items = vec![item];
    let to_info = ToInfo {
        address: "ckt1qyq8jy6e6hu89lzwwgv9qdx6p0kttl4uax9s79m0mr".to_string(),
        amount: "1111".to_string(),
    };
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByFrom,
        },
        pay_fee: None,
        change: None,
        fee_rate: None,
        since: None,
    };

    let ret = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await;
    println!("ret {:?}", ret);
    assert!(ret.is_err())
}

#[test]
async fn test_build_udt_cheque_transfer_transaction_with_source_claimable() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_udt(
        H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
    );
    let item = JsonItem::Identity("0x003534bd5b3fbcc7ae3ee7831ca0464a85789b2c58".to_string());
    let items = vec![item];
    let to_info = ToInfo {
        address: "ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsqdrhpvcu82numz73852ed45cdxn4kcn72cr4338a".to_string(),
        amount: "80".to_string(),
    };
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByFrom,
        },
        pay_fee: None,
        change: None,
        fee_rate: Some(1000),
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_udt_acp_transfer_transaction_with_change() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_udt(
        H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
    );
    let identity = new_identity_from_secp_address("ckt1qyq8jy6e6hu89lzwwgv9qdx6p0kttl4uax9s79m0mr");
    let item = JsonItem::Identity(hex::encode(identity.0));
    let items = vec![item];
    let to_info = ToInfo {
        address: "ckt1qypv2w7f5kuctnt03kk9l09gwuuy6wpys64smeamhm".to_string(),
        amount: "1111".to_string(),
    };
    let change = "ckt1qyqv2w7f5kuctnt03kk9l09gwuuy6wpys64s4f8vve".to_string();
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByTo,
        },
        pay_fee: None,
        change: Some(change),
        fee_rate: None,
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[ignore]
#[test]
async fn test_build_udt_acp_transfer_transaction_with_cheque_address() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_udt(
        H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
    );

    let item_cheque_address = JsonItem::Address("ckt1q3sdtuu7lnjqn3v8ew02xkwwlh4dv5x2z28shkwt8p2nfruccux4kaedejfkzfry4ccapp22qgsfr6schlz7aj5lc09uvu8xw3g7jg8x747xgl6jnet87rser4k".to_string());
    let items = vec![item_cheque_address];

    let to_info = ToInfo {
        address: "ckt1qypv2w7f5kuctnt03kk9l09gwuuy6wpys64smeamhm".to_string(),
        amount: "100".to_string(),
    };
    let pay_fee = "ckt1qyqv2w7f5kuctnt03kk9l09gwuuy6wpys64s4f8vve".to_string();
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByTo,
        },
        pay_fee: Some(pay_fee),
        change: None,
        fee_rate: None,
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[ignore]
#[test]
async fn test_build_udt_acp_transfer_transaction_with_cheque_record_id() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_udt(
        H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
    );

    // decode record_id
    let record_id_bytes = new_record_id(
        "52b1cf0ad857d53e1a3552944c1acf268f6a6aea8e8fc85fe8febcb8127d56f0",
        0,
        "ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsqtezdvat7rjl388yxzsxndqhm94l67wnzcfv52fg",
    );
    let record_id = hex::encode(record_id_bytes.to_vec());
    let item = JsonItem::Record(record_id);
    let items = vec![item];

    let to_info = ToInfo {
        address: "ckt1qypv2w7f5kuctnt03kk9l09gwuuy6wpys64smeamhm".to_string(),
        amount: "100".to_string(),
    };
    let pay_fee = "ckt1qyqv2w7f5kuctnt03kk9l09gwuuy6wpys64s4f8vve".to_string();
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByTo,
        },
        pay_fee: Some(pay_fee),
        change: None,
        fee_rate: None,
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_udt_acp_transfer_transaction_with_cheque_identity() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_udt(
        H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
    );

    // identity has cheque cells
    let identity = new_identity_from_secp_address("ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsqtezdvat7rjl388yxzsxndqhm94l67wnzcfv52fg");
    let item = JsonItem::Identity(hex::encode(identity.0));
    let items = vec![item];

    let to_info = ToInfo {
        address: "ckt1qypv2w7f5kuctnt03kk9l09gwuuy6wpys64smeamhm".to_string(),
        amount: "100".to_string(),
    };
    let pay_fee = "ckt1qyqv2w7f5kuctnt03kk9l09gwuuy6wpys64s4f8vve".to_string();
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByTo,
        },
        pay_fee: Some(pay_fee),
        change: None,
        fee_rate: None,
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[ignore]
#[test]
async fn test_build_udt_acp_transfer_transaction_with_cheque_address() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_udt(
        H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
    );

    let item_cheque_address = JsonItem::Address("ckt1q3sdtuu7lnjqn3v8ew02xkwwlh4dv5x2z28shkwt8p2nfruccux4kaedejfkzfry4ccapp22qgsfr6schlz7aj5lc09uvu8xw3g7jg8x747xgl6jnet87rser4k".to_string());
    let items = vec![item_cheque_address];

    let to_info = ToInfo {
        address: "ckt1qypv2w7f5kuctnt03kk9l09gwuuy6wpys64smeamhm".to_string(),
        amount: "100".to_string(),
    };
    let pay_fee = "ckt1qyqv2w7f5kuctnt03kk9l09gwuuy6wpys64s4f8vve".to_string();
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByTo,
        },
        pay_fee: Some(pay_fee),
        change: None,
        fee_rate: None,
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[ignore]
#[test]
async fn test_build_udt_acp_transfer_transaction_with_cheque_record_id() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_udt(
        H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
    );

    // decode record_id
    let record_id_bytes = new_record_id(
        "52b1cf0ad857d53e1a3552944c1acf268f6a6aea8e8fc85fe8febcb8127d56f0",
        0,
        "ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsqtezdvat7rjl388yxzsxndqhm94l67wnzcfv52fg",
    );
    let record_id = hex::encode(record_id_bytes.to_vec());
    let item = JsonItem::Record(record_id);
    let items = vec![item];

    let to_info = ToInfo {
        address: "ckt1qypv2w7f5kuctnt03kk9l09gwuuy6wpys64smeamhm".to_string(),
        amount: "100".to_string(),
    };
    let pay_fee = "ckt1qyqv2w7f5kuctnt03kk9l09gwuuy6wpys64s4f8vve".to_string();
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByTo,
        },
        pay_fee: Some(pay_fee),
        change: None,
        fee_rate: None,
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_udt_acp_transfer_transaction() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_udt(
        H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
    );

    let item = JsonItem::Address("ckt1qypyy66ugflt79v3udq049vre4g3gq7nw8zshxpfrg".to_string());
    let items = vec![item];

    let to_info = ToInfo {
        address: "ckt1qypv2w7f5kuctnt03kk9l09gwuuy6wpys64smeamhm".to_string(),
        amount: "1".to_string(),
    };
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByTo,
        },
        pay_fee: None,
        change: None,
        fee_rate: None,
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_udt_acp_transfer_transaction_from_pw_lock() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_udt(
        H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
    );
    let identity = new_identity_from_pw_lock_address("ckt1qpvvtay34wndv9nckl8hah6fzzcltcqwcrx79apwp2a5lkd07fdxxqdd40lmnsnukjh3qr88hjnfqvc4yg8g0gskp8ffv");
    let item = JsonItem::Identity(hex::encode(identity.0));
    let items = vec![item];
    let to_info = ToInfo {
        address: "ckt1qypv2w7f5kuctnt03kk9l09gwuuy6wpys64smeamhm".to_string(),
        amount: "100".to_string(),
    };
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByTo,
        },
        pay_fee: None,
        change: None,
        fee_rate: None,
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_udt_acp_transfer_transaction_to_pw_lock() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_udt(
        H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
    );
    let identity = new_identity_from_secp_address("ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsqwy7nwzv795wx5qc22rn9f9q58puwcrq3gxjcprk");
    let item = JsonItem::Identity(hex::encode(identity.0));
    let items = vec![item];
    let to_info = ToInfo {
        address: "ckt1qpvvtay34wndv9nckl8hah6fzzcltcqwcrx79apwp2a5lkd07fdxxqdd40lmnsnukjh3qr88hjnfqvc4yg8g0gskp8ffv".to_string(),
        amount: "100".to_string(),
    };
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByTo,
        },
        pay_fee: None,
        change: None,
        fee_rate: None,
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_udt_acp_transfer_transaction_with_pay_fee_with_change() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_udt(
        H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
    );
    let identity = new_identity_from_secp_address("ckt1qyq8jy6e6hu89lzwwgv9qdx6p0kttl4uax9s79m0mr");
    let item = JsonItem::Identity(hex::encode(identity.0));
    let items = vec![item];
    let to_info = ToInfo {
        address: "ckt1qypv2w7f5kuctnt03kk9l09gwuuy6wpys64smeamhm".to_string(),
        amount: "1111".to_string(),
    };
    let change = "ckt1qyqv2w7f5kuctnt03kk9l09gwuuy6wpys64s4f8vve".to_string();
    let pay_fee = "ckt1qyqyfy67hjrqmcyzs2cpvdfhd9lx6mgc68aqukw69v".to_string();
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::HoldByTo,
        },
        pay_fee: Some(pay_fee),
        change: Some(change),
        fee_rate: None,
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_udt_transfer_transaction_pay_with_acp() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_udt(
        H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
    );
    let identity = new_identity_from_secp_address("ckt1qyq8jy6e6hu89lzwwgv9qdx6p0kttl4uax9s79m0mr");
    let item = JsonItem::Identity(hex::encode(identity.0));
    let items = vec![item];
    let to_info = ToInfo {
        address: "ckt1qypv2w7f5kuctnt03kk9l09gwuuy6wpys64smeamhm".to_string(),
        amount: "1111".to_string(),
    };
    let payload = TransferPayload {
        asset_info,
        from: From2 {
            items,
            source: Source::Free,
        },
        to: To {
            to_infos: vec![to_info],
            mode: Mode::PayWithAcp,
        },
        pay_fee: None,
        change: None,
        fee_rate: None,
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_build_simple_transfer() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let asset_info = AssetInfo::new_udt(
        H256::from_str("f21e7350fa9518ed3cbb008e0e8c941d7e01a12181931d5608aa366ee22228bd").unwrap(),
    );
    let to_info = ToInfo {
        address: "ckt1qypg88ccqm59ksxp85788pnqg4rkejdgcg2qggxamt".to_string(),
        amount: "20".to_string(),
    };
    let payload = SimpleTransferPayload {
        asset_info,
        from: vec!["ckt1qyq27z6pccncqlaamnh8ttapwn260egnt67ss2cwvz".to_string()],
        to: vec![to_info],
        change: None,
        fee_rate: Some(1000),
        since: None,
    };

    let raw_transaction = rpc
        .inner_build_simple_transfer_transaction(Context::new(), payload)
        .await
        .unwrap();
    pretty_print_raw_tx(net_ty, &rpc, raw_transaction).await;
}

#[test]
async fn test_register_addresses() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let addresses = vec!["ckt1qyq8jy6e6hu89lzwwgv9qdx6p0kttl4uax9s79m0mr".to_string()];
    let raw_transaction = rpc.register_addresses(addresses).await.unwrap();
    let lock_hash = H160::from_str("ca9fc3cbc670e67451e920e6f57c647f529e567f").unwrap();
    assert_eq!(lock_hash, raw_transaction[0]);
}

#[test]
async fn test_register_addresses_pw_lock() {
    let net_ty = NetworkType::Testnet;
    let rpc = new_rpc(net_ty).await;
    init_tip(&rpc, None).await;

    let addresses = vec!["ckt1q3vvtay34wndv9nckl8hah6fzzcltcqwcrx79apwp2a5lkd07fdxxm88yfy8yaaspgy9922rhglatmsren9qvuknrnz".to_string()];
    let raw_transaction = rpc.register_addresses(addresses).await.unwrap();
    println!("raw_transaction: {:?}", raw_transaction);
}
