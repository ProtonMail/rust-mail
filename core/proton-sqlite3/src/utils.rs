use rusqlite::{MappedRows, Row};
use std::collections::{BTreeSet, HashSet};
use std::hash::Hash;

/// Generate a variable number of sql arguments form an IN query.
/// ```
/// use proton_sqlite3::utils::gen_variable_in_argument_list;
/// let query = format!("SELECT * FROM table WHERE id IN ({})", gen_variable_in_argument_list(5));
/// ```
/// The above snippet will print `SELECT * FROM table WHERE id IN (?,?,?,?,?)`.
pub fn gen_variable_in_argument_list(count: usize) -> String {
    debug_assert!(count > 0);
    let mut string = String::with_capacity(count + (count - 1));
    string.push('?');
    for _ in 1_usize..count {
        string.push_str(",?");
    }
    string
}

/// Convenience function to insert all mapped rows into an existing Vec or return error if the operation fails.
pub fn mapped_rows_into_vec<T, F: FnMut(&Row<'_>) -> rusqlite::Result<T>>(
    out: &mut Vec<T>,
    m: MappedRows<F>,
) -> rusqlite::Result<()> {
    for item in m {
        out.push(item?);
    }
    Ok(())
}

/// Convenience function to insert all mapped rows into a Vec or return error if the operation fails.
pub fn mapped_rows_to_vec<T, F: FnMut(&Row<'_>) -> rusqlite::Result<T>>(
    m: MappedRows<F>,
) -> rusqlite::Result<Vec<T>> {
    let mut vec = Vec::new();
    mapped_rows_into_vec(&mut vec, m)?;
    Ok(vec)
}

/// Convenience function to insert all mapped rows into an existing BTreeSet or return error if the operation fails.
pub fn mapped_rows_into_btree_set<
    T: PartialOrd + PartialEq + Ord + Eq,
    F: FnMut(&Row<'_>) -> rusqlite::Result<T>,
>(
    out: &mut BTreeSet<T>,
    m: MappedRows<F>,
) -> rusqlite::Result<()> {
    for item in m {
        out.insert(item?);
    }
    Ok(())
}

/// Convenience function to insert all mapped rows into a BTreeSet or return error if the operation fails.
pub fn mapped_rows_to_btree_set<
    T: PartialOrd + PartialEq + Ord + Eq,
    F: FnMut(&Row<'_>) -> rusqlite::Result<T>,
>(
    m: MappedRows<F>,
) -> rusqlite::Result<BTreeSet<T>> {
    let mut btree = BTreeSet::new();
    mapped_rows_into_btree_set(&mut btree, m)?;
    Ok(btree)
}

/// Convenience function to insert all mapped rows into an existing HashSet or return error if the operation fails.
pub fn mapped_rows_into_hash_set<
    T: Hash + PartialEq + Eq,
    F: FnMut(&Row<'_>) -> rusqlite::Result<T>,
>(
    out: &mut HashSet<T>,
    m: MappedRows<F>,
) -> rusqlite::Result<()> {
    for item in m {
        out.insert(item?);
    }
    Ok(())
}

/// Convenience function to insert all mapped rows into a HashSet or return error if the operation fails.
pub fn mapped_rows_to_hash_set<
    T: Hash + PartialEq + Eq,
    F: FnMut(&Row<'_>) -> rusqlite::Result<T>,
>(
    m: MappedRows<F>,
) -> rusqlite::Result<HashSet<T>> {
    let mut hash_set = HashSet::new();
    mapped_rows_into_hash_set(&mut hash_set, m)?;
    Ok(hash_set)
}
