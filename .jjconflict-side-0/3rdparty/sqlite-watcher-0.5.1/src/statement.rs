use crate::connection::{SqlExecutor, SqlExecutorAsync, SqlExecutorMut};
use std::future::Future;
use tracing::{Instrument, Span, error};

pub(super) trait Sealed {}

/// Basic abstraction that defers the execution of a SQL statement in order to reduce the duplication
/// of sync and async code. Basic composability and chaining are also included.
///
/// The `Send` requirement is in theory not required for sync implementations, but this is not
/// intended to be used outside of the scope of this crate.
#[allow(private_bounds)]
pub trait Statement: Send + Sealed {
    /// Output of this statement.
    type Output: Send;

    /// Execute the statement and return the result.
    ///
    /// # Errors
    ///
    /// If the statement fails, return error.
    fn execute<S: SqlExecutor>(self, connection: &S) -> Result<Self::Output, S::Error>;

    /// Execute the statement and return the result.
    ///
    /// # Errors
    ///
    /// If the statement fails, return error.
    fn execute_mut<S: SqlExecutorMut>(self, connection: &mut S) -> Result<Self::Output, S::Error>;

    /// Execute the statement and return the result.
    ///
    /// # Errors
    ///
    /// If the statement fails, return error.
    fn execute_async<S: SqlExecutorAsync>(
        self,
        connection: &mut S,
    ) -> impl Future<Output = Result<Self::Output, S::Error>> + Send;

    /// If this statement succeeds, then execute the next `statement`.
    fn then<Q: Statement>(self, statement: Q) -> Then<Self, Q>
    where
        Self: Sized,
    {
        Then {
            a: self,
            b: statement,
        }
    }

    /// if the current statement succeeds, then execute the next `statement` with output of the
    /// current [`Statement`].
    fn pipe<Q: StatementWithInput<Input = Self::Output> + Send>(self, statement: Q) -> Pipe<Self, Q>
    where
        Self: Sized,
    {
        Pipe {
            a: self,
            b: statement,
        }
    }

    /// Instrument the current statement with the given [`Span`]
    fn spanned(self, span: Span) -> TracedStatement<Self>
    where
        Self: Sized,
    {
        TracedStatement::new(self, span)
    }

    /// Instrument the current statement with currently active [`Span`]
    fn spanned_in_current(self) -> TracedStatement<Self>
    where
        Self: Sized,
    {
        TracedStatement::current(self)
    }
}

/// Similar to [`Statement`] but accepts an input parameter.
///
/// This statement is intended to be used with [`Statement::pipe`].
pub trait StatementWithInput: Send {
    /// Input for the statement.
    type Input: Send;
    /// Output of the statement.
    type Output: Send;

    /// Execute the statement with the given `input` and return the result.
    ///
    /// # Errors
    ///
    /// If the statement fails, return error.
    fn execute<S: SqlExecutor>(
        self,
        connection: &S,
        input: Self::Input,
    ) -> Result<Self::Output, S::Error>;

    /// Execute the statement with the given `input` and return the result.
    ///
    /// # Errors
    ///
    /// If the statement fails, return error.
    fn execute_mut<S: SqlExecutorMut>(
        self,
        connection: &mut S,
        input: Self::Input,
    ) -> Result<Self::Output, S::Error>;

    /// Execute the statement with the given `input` and return the result.
    ///
    /// # Errors
    ///
    /// If the statement fails, return error.
    fn execute_async<S: SqlExecutorAsync>(
        self,
        connection: &mut S,
        input: Self::Input,
    ) -> impl Future<Output = Result<Self::Output, S::Error>> + Send;
}

/// Link two [`Statement`]s.
///
/// Statement `B` is only executed if `A` fails.
pub struct Then<A: Statement, B: Statement> {
    a: A,
    b: B,
}
impl<A: Statement, B: Statement> Sealed for Then<A, B> {}

impl<A: Statement + Send, B: Statement + Send> Statement for Then<A, B> {
    type Output = B::Output;

    fn execute<S: SqlExecutor>(self, connection: &S) -> Result<Self::Output, S::Error> {
        self.a.execute(connection)?;
        self.b.execute(connection)
    }

    fn execute_mut<S: SqlExecutorMut>(self, connection: &mut S) -> Result<Self::Output, S::Error> {
        self.a.execute_mut(connection)?;
        self.b.execute_mut(connection)
    }

    async fn execute_async<S: SqlExecutorAsync>(
        self,
        connection: &mut S,
    ) -> Result<Self::Output, S::Error> {
        self.a.execute_async(connection).await?;
        self.b.execute_async(connection).await
    }
}

/// Link two [`Statement`]s and use the output of `A` as the input of `B`.
pub struct Pipe<A: Statement + Send, B: StatementWithInput<Input = A::Output> + Send> {
    a: A,
    b: B,
}

impl<A: Statement, B: StatementWithInput<Input = A::Output>> Sealed for Pipe<A, B> {}
impl<A: Statement + Send, B: StatementWithInput<Input = A::Output> + Send> Statement
    for Pipe<A, B>
{
    type Output = B::Output;

    fn execute<S: SqlExecutor>(self, connection: &S) -> Result<Self::Output, S::Error> {
        let output = self.a.execute(connection)?;
        self.b.execute(connection, output)
    }

    fn execute_mut<S: SqlExecutorMut>(self, connection: &mut S) -> Result<Self::Output, S::Error> {
        let output = self.a.execute_mut(connection)?;
        self.b.execute_mut(connection, output)
    }

    async fn execute_async<S: SqlExecutorAsync>(
        self,
        connection: &mut S,
    ) -> Result<Self::Output, S::Error> {
        let output = self.a.execute_async(connection).await?;
        self.b.execute_async(connection, output).await
    }
}

/// Execute an SQL statement which does not return any value.
pub(super) struct SqlExecuteStatement<T: AsRef<str>> {
    query: T,
}

impl<T: AsRef<str> + Send> SqlExecuteStatement<T> {
    pub fn new(query: T) -> Self {
        Self { query }
    }
}

impl<T: AsRef<str> + Send> Sealed for SqlExecuteStatement<T> {}

impl<T: AsRef<str> + Send> Statement for SqlExecuteStatement<T> {
    type Output = ();

    fn execute<S: SqlExecutor>(self, connection: &S) -> Result<Self::Output, S::Error> {
        connection.sql_execute(self.query.as_ref())
    }

    fn execute_mut<S: SqlExecutorMut>(self, connection: &mut S) -> Result<Self::Output, S::Error> {
        connection.sql_execute(self.query.as_ref())
    }

    async fn execute_async<S: SqlExecutorAsync>(
        self,
        connection: &mut S,
    ) -> Result<Self::Output, S::Error> {
        connection.sql_execute(self.query.as_ref()).await
    }
}

/// Controls the transaction behavior.
enum TransactionMode {
    /// Locks only temporary tables. Note that this not guaranteed if the transaction touches
    /// other tables that are not temporary
    Temporary,
    /// Locks the full database.
    Full,
}

/// Execute an SQL Transaction.
pub(super) struct SqlTransactionStatement<Q: Statement> {
    statement: Q,
    mode: TransactionMode,
}

impl<Q: Statement> SqlTransactionStatement<Q> {
    /// Create new transaction that only affects temporary tables.
    pub fn temporary(statement: Q) -> Self {
        Self {
            statement,
            mode: TransactionMode::Temporary,
        }
    }
    /// Create a new transaction that affects all tables.
    #[allow(dead_code)]
    pub fn full(statement: Q) -> Self {
        Self {
            statement,
            mode: TransactionMode::Full,
        }
    }

    fn begin_statement(&self) -> &'static str {
        match self.mode {
            TransactionMode::Temporary => BEGIN_TRANSACTION_STATEMENT,
            TransactionMode::Full => BEGIN_TRANSACTION_IMMEDIATE_STATEMENT,
        }
    }
}

impl<Q: Statement<Output = ()>> Sealed for SqlTransactionStatement<Q> {}

impl<Q: Statement<Output = ()>> Statement for SqlTransactionStatement<Q> {
    type Output = ();

    fn execute<S: SqlExecutor>(self, connection: &S) -> Result<Self::Output, S::Error> {
        connection
            .sql_execute(self.begin_statement())
            .inspect_err(|e| error!("Failed to start transaction: {e}"))?;
        if let Err(e) = self.statement.execute(connection) {
            error!("Statement failed to execute: {e}");
            if let Err(e) = connection.sql_execute(ROLLBACK_TRANSACTION_STATEMENT) {
                error!("Failed to rollback transaction: {e}");
            }
            return Err(e);
        }
        connection
            .sql_execute(COMMIT_TRANSACTION_STATEMENT)
            .inspect_err(|e| error!("Failed to commit transaction: {e}"))?;
        Ok(())
    }

    fn execute_mut<S: SqlExecutorMut>(self, connection: &mut S) -> Result<Self::Output, S::Error> {
        connection
            .sql_execute(self.begin_statement())
            .inspect_err(|e| error!("Failed to start transaction: {e}"))?;
        if let Err(e) = self.statement.execute_mut(connection) {
            error!("Statement failed to execute: {e}");
            if let Err(e) = connection.sql_execute(ROLLBACK_TRANSACTION_STATEMENT) {
                error!("Failed to rollback transaction: {e}");
            }
            return Err(e);
        }
        connection
            .sql_execute(COMMIT_TRANSACTION_STATEMENT)
            .inspect_err(|e| error!("Failed to commit transaction: {e}"))?;
        Ok(())
    }
    async fn execute_async<S: SqlExecutorAsync>(
        self,
        connection: &mut S,
    ) -> Result<Self::Output, S::Error> {
        connection
            .sql_execute(self.begin_statement())
            .await
            .inspect_err(|e| error!("Failed to start transaction: {e}"))?;
        if let Err(e) = self.statement.execute_async(connection).await {
            error!("Statement failed to execute: {e}");
            if let Err(e) = connection.sql_execute(ROLLBACK_TRANSACTION_STATEMENT).await {
                error!("Failed to rollback transaction: {e}");
            }
            return Err(e);
        }
        connection
            .sql_execute(COMMIT_TRANSACTION_STATEMENT)
            .await
            .inspect_err(|e| error!("Failed to commit transaction: {e}"))?;
        Ok(())
    }
}

/// Execute a collections of [`Statement`].
///
/// Execution will halt on the first failed statement.
pub(super) struct BatchQuery<Q: Statement>(Vec<Q>);

impl<Q: Statement> BatchQuery<Q> {
    pub fn new(v: impl IntoIterator<Item = Q>) -> Self {
        Self(Vec::from_iter(v))
    }

    pub fn push(&mut self, q: Q) {
        self.0.push(q);
    }

    pub fn extend<I: IntoIterator<Item = Q>>(&mut self, iter: I) {
        self.0.extend(iter);
    }
}

impl<Q: Statement<Output = ()>> Sealed for BatchQuery<Q> {}

impl<Q: Statement<Output = ()>> Statement for BatchQuery<Q> {
    type Output = ();

    fn execute<S: SqlExecutor>(self, connection: &S) -> Result<Self::Output, S::Error> {
        for q in self.0 {
            q.execute(connection)?;
        }
        Ok(())
    }

    fn execute_mut<S: SqlExecutorMut>(self, connection: &mut S) -> Result<Self::Output, S::Error> {
        for q in self.0 {
            q.execute_mut(connection)?;
        }
        Ok(())
    }
    async fn execute_async<S: SqlExecutorAsync>(
        self,
        connection: &mut S,
    ) -> Result<Self::Output, S::Error> {
        for q in self.0 {
            q.execute_async(connection).await?;
        }
        Ok(())
    }
}

impl<Q: Statement> Sealed for Option<Q> {}

impl<Q: Statement> Statement for Option<Q> {
    type Output = Option<Q::Output>;

    fn execute<S: SqlExecutor>(self, connection: &S) -> Result<Self::Output, S::Error> {
        Ok(match self {
            Some(q) => Some(q.execute(connection)?),
            None => None,
        })
    }

    fn execute_mut<S: SqlExecutorMut>(self, connection: &mut S) -> Result<Self::Output, S::Error> {
        Ok(match self {
            Some(q) => Some(q.execute_mut(connection)?),
            None => None,
        })
    }

    async fn execute_async<S: SqlExecutorAsync>(
        self,
        connection: &mut S,
    ) -> Result<Self::Output, S::Error> {
        Ok(match self {
            Some(q) => Some(q.execute_async(connection).await?),
            None => None,
        })
    }
}

pub struct TracedStatement<Q: Statement> {
    statement: Q,
    span: Span,
}

impl<Q: Statement> TracedStatement<Q> {
    /// Create a new traced `span` for the `statement`.
    pub fn new(statement: Q, span: Span) -> Self {
        Self { statement, span }
    }

    /// Create a new traced span for the `statement` using the current active tracing span.
    pub fn current(statement: Q) -> Self {
        Self::new(statement, Span::current())
    }
}

impl<Q: Statement> Sealed for TracedStatement<Q> {}

impl<Q: Statement> Statement for TracedStatement<Q> {
    type Output = Q::Output;
    fn execute<S: SqlExecutor>(self, connection: &S) -> Result<Self::Output, S::Error> {
        let _span = self.span.entered();
        self.statement.execute(connection)
    }

    fn execute_mut<S: SqlExecutorMut>(self, connection: &mut S) -> Result<Self::Output, S::Error> {
        let _span = self.span.entered();
        self.statement.execute_mut(connection)
    }

    async fn execute_async<S: SqlExecutorAsync>(
        self,
        connection: &mut S,
    ) -> Result<Self::Output, S::Error> {
        self.statement
            .execute_async(connection)
            .instrument(self.span)
            .await
    }
}

const BEGIN_TRANSACTION_STATEMENT: &str = "BEGIN";
const BEGIN_TRANSACTION_IMMEDIATE_STATEMENT: &str = "BEGIN IMMEDIATE";
const COMMIT_TRANSACTION_STATEMENT: &str = "COMMIT";
const ROLLBACK_TRANSACTION_STATEMENT: &str = "ROLLBACK";
