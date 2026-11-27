use super::{AppError, AppState, Body, CurrentUser, Deserialize, Result, State, StatusCode};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Deserialize)]
pub(super) struct Interval {
    start_ms: i64,
    end_ms: i64,
}

pub(super) async fn record(
    State(s): State<AppState>,
    user: CurrentUser,
    Body(span): Body<Interval>,
) -> Result<StatusCode> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AppError::internal())?
        .as_millis() as i64;
    if span.start_ms < now - 300_000
        || span.end_ms > now + 5_000
        || span.end_ms <= span.start_ms
        || span.end_ms.saturating_sub(span.start_ms) > 30_000
    {
        return Err(AppError::bad_request(
            "invalid_interval",
            "Send a recent practice interval of up to 30 seconds",
        ));
    }
    // Split at UTC midnight so overlapping requests always lock the same day rows in order.
    s.pool.get().await?.execute(
        "INSERT INTO study_time (user_id, utc_day, intervals)
         SELECT $1, day, int8multirange(int8range(greatest($2::bigint, day*86400000),
                                                  least($3::bigint, (day+1)*86400000), '[)'))
         FROM generate_series($2::bigint/86400000, ($3::bigint-1)/86400000) day ORDER BY day
         ON CONFLICT (user_id, utc_day) DO UPDATE SET intervals = study_time.intervals + excluded.intervals",
        &[&user.id, &span.start_ms, &span.end_ms],
    ).await?;
    Ok(StatusCode::NO_CONTENT)
}
