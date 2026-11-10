//! User-defined groups for labeling network subnets.

use chrono::{DateTime, Utc};
use sqlx::{FromRow, SqliteExecutor, SqlitePool};

use crate::error::DatabaseError;

/// Stored group row.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct GroupRow {
    pub id: i64,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

/// Group row with member subnet count for the Groups page.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct GroupListRow {
    pub id: i64,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub network_count: i64,
}

/// Network member of a group.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct GroupNetworkRow {
    pub id: i64,
    pub cidr: String,
    pub net_name: Option<String>,
    pub organization: Option<String>,
}

/// Creates a group by unique name. Returns the new or existing id when the name already exists.
pub async fn create_group(
    pool: &SqlitePool,
    name: &str,
    created_at: DateTime<Utc>,
) -> Result<i64, DatabaseError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(DatabaseError::Message("group name is empty".into()));
    }

    let id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO "groups" (name, created_at)
        VALUES (?1, ?2)
        ON CONFLICT (name) DO UPDATE SET name = excluded.name
        RETURNING id
        "#,
    )
    .bind(trimmed)
    .bind(created_at)
    .fetch_one(pool)
    .await?;

    Ok(id)
}

/// Loads a group by primary key.
pub async fn get_group<'e, E>(executor: E, id: i64) -> Result<Option<GroupRow>, DatabaseError>
where
    E: SqliteExecutor<'e>,
{
    let row = sqlx::query_as::<_, GroupRow>(
        r#"
        SELECT id, name, created_at
        FROM "groups"
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(executor)
    .await?;
    Ok(row)
}

/// Loads a group by exact name.
pub async fn get_group_by_name<'e, E>(
    executor: E,
    name: &str,
) -> Result<Option<GroupRow>, DatabaseError>
where
    E: SqliteExecutor<'e>,
{
    let row = sqlx::query_as::<_, GroupRow>(
        r#"
        SELECT id, name, created_at
        FROM "groups"
        WHERE name = ?1
        "#,
    )
    .bind(name.trim())
    .fetch_optional(executor)
    .await?;
    Ok(row)
}

/// Renames a group. Fails when the new name is empty or already taken by another group.
pub async fn rename_group(
    pool: &SqlitePool,
    id: i64,
    new_name: &str,
) -> Result<(), DatabaseError> {
    let trimmed = new_name.trim();
    if trimmed.is_empty() {
        return Err(DatabaseError::Message("group name is empty".into()));
    }

    let result = sqlx::query(
        r#"
        UPDATE "groups"
        SET name = ?2
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .bind(trimmed)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(DatabaseError::Message(format!("group {id} not found")));
    }
    Ok(())
}

/// Deletes a group. Member networks keep their rows; `group_id` is cleared by FK.
pub async fn delete_group(pool: &SqlitePool, id: i64) -> Result<(), DatabaseError> {
    let result = sqlx::query(r#"DELETE FROM "groups" WHERE id = ?1"#)
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(DatabaseError::Message(format!("group {id} not found")));
    }
    Ok(())
}

/// Lists all groups with subnet counts, ordered by name.
pub async fn list_groups(pool: &SqlitePool) -> Result<Vec<GroupListRow>, DatabaseError> {
    let rows = sqlx::query_as::<_, GroupListRow>(
        r#"
        SELECT
            g.id AS id,
            g.name AS name,
            g.created_at AS created_at,
            COUNT(n.id) AS network_count
        FROM "groups" g
        LEFT JOIN networks n ON n.group_id = g.id
        GROUP BY g.id, g.name, g.created_at
        ORDER BY g.name ASC
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Lists networks that belong to `group_id`.
pub async fn list_networks_for_group(
    pool: &SqlitePool,
    group_id: i64,
) -> Result<Vec<GroupNetworkRow>, DatabaseError> {
    let rows = sqlx::query_as::<_, GroupNetworkRow>(
        r#"
        SELECT id, cidr, net_name, organization
        FROM networks
        WHERE group_id = ?1
        ORDER BY cidr ASC
        "#,
    )
    .bind(group_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Assigns or clears the group on a network row.
pub async fn set_network_group(
    pool: &SqlitePool,
    network_id: i64,
    group_id: Option<i64>,
) -> Result<(), DatabaseError> {
    let result = sqlx::query(
        r#"
        UPDATE networks
        SET group_id = ?2
        WHERE id = ?1
        "#,
    )
    .bind(network_id)
    .bind(group_id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(DatabaseError::Message(format!(
            "network {network_id} not found"
        )));
    }
    Ok(())
}

/// Sets `group_id` on every network whose CIDR is in `cidrs`.
pub async fn set_networks_group_by_cidr(
    pool: &SqlitePool,
    cidrs: &[String],
    group_id: Option<i64>,
) -> Result<u64, DatabaseError> {
    if cidrs.is_empty() {
        return Ok(0);
    }

    let mut affected = 0u64;
    for cidr in cidrs {
        let result = sqlx::query(
            r#"
            UPDATE networks
            SET group_id = ?2
            WHERE cidr = ?1
            "#,
        )
        .bind(cidr)
        .bind(group_id)
        .execute(pool)
        .await?;
        affected += result.rows_affected();
    }
    Ok(affected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    use crate::models::NewNetwork;
    use crate::writes::upsert_network;
    use crate::Database;

    fn t() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap()
    }

    #[tokio::test]
    async fn group_crud_and_network_assignment() {
        let db = Database::open_in_memory().await.unwrap();
        let group_id = create_group(db.pool(), "OpenAI", t()).await.unwrap();

        let network_id = upsert_network(
            db.pool(),
            &NewNetwork {
                cidr: "203.0.113.0/24".into(),
                net_range: None,
                net_name: Some("TEST".into()),
                organization: None,
                asn: None,
                country: None,
                source: Some("manual".into()),
                updated_at: t(),
            },
        )
        .await
        .unwrap();

        set_network_group(db.pool(), network_id, Some(group_id))
            .await
            .unwrap();

        let groups = list_groups(db.pool()).await.unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "OpenAI");
        assert_eq!(groups[0].network_count, 1);

        let members = list_networks_for_group(db.pool(), group_id).await.unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].cidr, "203.0.113.0/24");

        rename_group(db.pool(), group_id, "ChatGPT").await.unwrap();
        let renamed = get_group(db.pool(), group_id).await.unwrap().unwrap();
        assert_eq!(renamed.name, "ChatGPT");

        delete_group(db.pool(), group_id).await.unwrap();
        assert!(list_groups(db.pool()).await.unwrap().is_empty());

        // Network remains; group_id cleared by FK ON DELETE SET NULL.
        let cidr: Option<String> = sqlx::query_scalar(
            "SELECT cidr FROM networks WHERE id = ?1 AND group_id IS NULL",
        )
        .bind(network_id)
        .fetch_optional(db.pool())
        .await
        .unwrap();
        assert_eq!(cidr.as_deref(), Some("203.0.113.0/24"));
    }
}
