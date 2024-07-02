use crossbeam::channel::Sender;
use diesel::PgConnection;
use diesel::r2d2::ConnectionManager;
use dotenvy_macro::dotenv;
use fatline_rs::HubService;
use r2d2_postgres::r2d2::Pool;

use crate::worker::Task;

pub struct ServiceState {
    pub(crate) hub_client: HubService,
    pub(crate) db_pool: DbPool,
    pub(crate) work_sender: Sender<Task>,
}

pub type DbPool = Pool<ConnectionManager<PgConnection>>;

const DB_URL:&'static str = dotenv!("DATABASE_URL");
const HUB_URL: &'static str = dotenv!("SERVER_URL");

impl ServiceState {

    pub async fn hub_client() -> HubService {
        HubService::connect(HUB_URL).await.expect("Couldn't build hub client")
    }

    pub async fn db_pool(max_size: u32) -> DbPool {
        let pg_connection = ConnectionManager::new(DB_URL);
        Pool::builder()
            .max_size(max_size)
            .build(pg_connection).expect("Couldn't create pg pool")
    }

    pub async fn new(sender: Sender<Task>) -> Self {
        let client = Self::hub_client().await;
        let pool = Self::db_pool(4).await;

        Self {
            hub_client: client,
            db_pool: pool,
            work_sender: sender
        }
    }
}
