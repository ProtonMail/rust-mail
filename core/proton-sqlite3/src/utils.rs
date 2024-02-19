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
