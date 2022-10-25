mod sync_test;

use crate::SyncAdapter;

use common::{async_trait, Order, PaginationRequest, Range, Result};
use core_rpc_types::IOType;
use core_storage::{relational::RelationalStorage, DBDriver};
use xsql_test::read_block_view;

use ckb_types::core::{BlockNumber, BlockView};

const MEMORY_DB: &str = ":memory:";
const BLOCK_DIR: &str = "../../devtools/test_data/blocks/";

async fn connect_sqlite() -> Result<RelationalStorage> {
    let mut pool = RelationalStorage::new(0, 0, 1, 0, 60, 1800, 30);
    pool.connect(DBDriver::SQLite, MEMORY_DB, "", 0, "", "")
        .await?;
    Ok(pool)
}

async fn connect_and_create_tables() -> Result<RelationalStorage> {
    let pool = connect_sqlite().await?;
    let tx = pool.sqlx_pool.transaction().await?;
    xsql_test::create_tables(tx).await?;
    Ok(pool)
}

#[derive(Clone, Debug)]
pub struct CkbRpcTestClient;

#[async_trait]
impl SyncAdapter for CkbRpcTestClient {
    async fn pull_blocks(&self, _block_numbers: Vec<BlockNumber>) -> Result<Vec<BlockView>> {
        let ret = (0..10)
            .map(|i| {
                let block_view: BlockView = read_block_view(i, String::from(BLOCK_DIR)).into();
                block_view
            })
            .into_iter()
            .collect();
        Ok(ret)
    }
}