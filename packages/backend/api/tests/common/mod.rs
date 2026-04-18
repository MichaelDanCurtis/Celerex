use testcontainers::{ContainerAsync, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use sqlx::PgPool;

pub struct TestDb {
    pub pool: PgPool,
    _container: ContainerAsync<Postgres>,
}

pub async fn start_test_db() -> TestDb {
    // SAFETY: tests run in a single process; set before any route handler reads env.
    unsafe { std::env::set_var("CELEREX_CRED_KEY", "0123456789abcdef0123456789abcdef"); }
    let container = Postgres::default().start().await.expect("postgres");
    let port = container.get_host_port_ipv4(5432).await.expect("port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPool::connect(&url).await.expect("connect");
    sqlx::migrate!("./src/db/migrations").run(&pool).await.expect("migrate");
    TestDb { pool, _container: container }
}
