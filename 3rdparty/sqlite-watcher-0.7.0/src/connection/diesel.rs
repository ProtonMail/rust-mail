use crate::connection::SqlExecutorMut;
use diesel::{RunQueryDsl, SqliteConnection, sql_query};

impl SqlExecutorMut for SqliteConnection {
    type Error = diesel::result::Error;

    // the type is supposed to be u32, but diesel does not have deserialization option for
    // u32.
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    fn sql_query_values(&mut self, query: &str) -> Result<Vec<u32>, Self::Error> {
        let values: Vec<Table> = sql_query(query).get_results(self)?;
        Ok(values.into_iter().map(|v| v.table_id as u32).collect())
    }

    fn sql_execute(&mut self, query: &str) -> Result<(), Self::Error> {
        sql_query(query).execute(self)?;
        Ok(())
    }
}
#[derive(diesel::QueryableByName, PartialOrd, PartialEq, Debug)]
#[diesel(table_name=table_ids)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct Table {
    table_id: i64,
}

diesel::table! {
    table_ids(table_id) {
        table_id -> BigInt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::test::TestObserver;
    use crate::watcher::Watcher;
    use diesel::dsl::{delete, insert_into};
    use diesel::prelude::*;
    use diesel::{Connection, result};
    use std::collections::BTreeSet;
    use std::sync::Arc;

    #[test]
    fn transaction_tracking() {
        let mut connection = SqliteConnection::establish(":memory:").unwrap();

        sql_query("CREATE TABLE foo (id INTEGER PRIMARY KEY AUTOINCREMENT, v INTEGER)")
            .execute(&mut connection)
            .unwrap();
        sql_query("CREATE TABLE bar (v INTEGER UNIQUE)")
            .execute(&mut connection)
            .unwrap();

        let watcher = Watcher::new().unwrap();
        let mut connection =
            crate::connection::Connection::new(connection, Arc::clone(&watcher)).unwrap();
        let foo_table_set = BTreeSet::from_iter(["foo".to_string()]);
        let bar_table_set = BTreeSet::from_iter(["bar".to_string()]);
        let foo_bar_table_set = BTreeSet::from_iter(["foo".to_string(), "bar".to_string()]);

        // Synchronization to avoid merging of changes;
        let (observer, receiver) = TestObserver::new(
            foo_bar_table_set.clone().into_iter().collect(),
            [
                foo_table_set,
                bar_table_set.clone(),
                bar_table_set,
                foo_bar_table_set,
            ],
        );

        let _ = watcher.add_observer(Box::new(observer));

        do_tx(&mut connection, |tx| {
            insert_into(foo::dsl::foo)
                .values(Foo { v: 10 })
                .execute(tx)
                .unwrap();
        });
        receiver.recv().unwrap();
        do_tx(&mut connection, |tx| {
            let _ = insert_into(bar::dsl::bar)
                .values(Bar { v: 10 })
                .on_conflict(bar::v)
                .do_update()
                .set(bar::v::eq(bar::columns::v, 10))
                .execute(tx)
                .unwrap();
        });
        receiver.recv().unwrap();
        do_tx(&mut connection, |tx| {
            let _ = insert_into(bar::dsl::bar)
                .values(Bar { v: 10 })
                .on_conflict(bar::v)
                .do_update()
                .set(bar::v::eq(bar::columns::v, 10))
                .execute(tx)
                .unwrap();
        });
        receiver.recv().unwrap();
        do_tx(&mut connection, |tx| {
            delete(foo::dsl::foo)
                .filter(foo::dsl::v.eq(10))
                .execute(tx)
                .unwrap();
            delete(bar::dsl::bar)
                .filter(bar::dsl::v.eq(10))
                .execute(tx)
                .unwrap();
        });
        receiver.recv().unwrap();

        connection.stop_tracking().unwrap();
    }

    fn do_tx(
        connection: &mut crate::connection::Connection<SqliteConnection>,
        f: impl FnOnce(&mut SqliteConnection),
    ) {
        connection.sync_watcher_tables().unwrap();
        connection
            .connection
            .immediate_transaction(|tx| {
                f(tx);
                Ok::<_, result::Error>(())
            })
            .unwrap();
        connection.publish_watcher_changes().unwrap();
    }

    #[derive(diesel::Insertable)]
    #[diesel(table_name = foo)]
    struct Foo {
        v: i32,
    }

    #[derive(diesel::Insertable)]
    #[diesel(table_name = bar)]
    struct Bar {
        v: i32,
    }

    diesel::table! {
        foo(id) {
            id -> Integer,
            v -> Integer,
        }
    }

    diesel::table! {
        bar (v) {
            v -> Integer,
        }
    }
}
