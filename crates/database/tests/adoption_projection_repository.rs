use sqlx::{postgres::PgPoolOptions, query_scalar};
use techatlas_database::{
    PostgresAdoptionProjectionRepository, PostgresPublicReadRepository, run_migrations,
};
use techatlas_models::{
    AdoptionProjectionOperations, PublicAdoptionHistoryStatus, PublicReadOperations,
};
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{ImageExt, runners::AsyncRunner},
};
use time::OffsetDateTime;

#[tokio::test]
async fn adoption_projection_is_idempotent_rebuildable_and_reports_history_coverage()
-> Result<(), Box<dyn std::error::Error>> {
    let container = Postgres::default().with_tag("16-alpine").start().await?;
    let port = container.get_host_port_ipv4(5432).await?;
    let database_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await?;
    run_migrations(&pool).await?;

    let projection = PostgresAdoptionProjectionRepository::new(pool.clone());
    let reads = PostgresPublicReadRepository::new(pool.clone());
    let now = OffsetDateTime::now_utc();
    let first_day = now.date();
    let second_day = first_day.next_day().ok_or("date should have a next day")?;
    let empty_day = second_day.next_day().ok_or("date should have a next day")?;

    let empty = reads.adoption_history(empty_day, None).await?;
    assert_eq!(
        empty.status,
        PublicAdoptionHistoryStatus::InsufficientHistory
    );
    assert!(empty.series.is_empty());

    let first_capture = projection.capture_daily_adoption(first_day, now).await?;
    assert!(!first_capture.already_captured);
    assert!(first_capture.inserted_rows > 0);
    let repeated_capture = projection.capture_daily_adoption(first_day, now).await?;
    assert!(repeated_capture.already_captured);
    assert_eq!(repeated_capture.inserted_rows, 0);

    let single_point = reads.adoption_history(first_day, None).await?;
    assert_eq!(
        single_point.status,
        PublicAdoptionHistoryStatus::InsufficientHistory
    );
    assert!(!single_point.series.is_empty());

    projection.capture_daily_adoption(second_day, now).await?;
    let multi_point = reads.adoption_history(first_day, Some("nextjs")).await?;
    assert_eq!(multi_point.status, PublicAdoptionHistoryStatus::Ready);
    let nextjs = multi_point
        .series
        .iter()
        .find(|series| series.slug == "nextjs")
        .ok_or("requested technology should be returned")?;
    assert_eq!(nextjs.points.len(), 2);

    let rebuild = projection
        .rebuild_adoption_history(first_day, first_day, now)
        .await?;
    assert_eq!(rebuild.rebuilt_days, 1);
    assert!(rebuild.inserted_rows > 0);
    assert_eq!(
        query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM technology_adoption_daily WHERE observed_on = $1",
        )
        .bind(first_day)
        .fetch_one(&pool)
        .await?,
        i64::try_from(rebuild.inserted_rows)?
    );

    pool.close().await;
    Ok(())
}
