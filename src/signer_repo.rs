use axum::async_trait;
use diesel::dsl::insert_into;
use diesel::prelude::*;
use diesel::result::Error;
use eyre::bail;
use tracing::error;
use crate::error::ServerError;
use crate::schema::signers::dsl::signers;
use crate::schema::signers::pk;
use crate::schema::users::dsl::users;
use crate::service::ServiceState;
use crate::user_models::{Signer, User};

#[async_trait]
pub trait SignerRepository {
    async fn get_signer(&mut self, pk_q: Vec<u8>) -> eyre::Result<Option<Signer>>;
    async fn insert_signer(&mut self, signer: Signer) -> eyre::Result<Signer>;
}

#[async_trait]
impl SignerRepository for ServiceState {
    async fn get_signer(&mut self, pk_q: Vec<u8>) -> eyre::Result<Option<Signer>> {
        let mut db = self.db_pool.get()?;
        let existing = signers.select(Signer::as_select()).filter(pk.eq(pk_q)).get_result(&mut db).optional();
        match existing {
            Ok(Some(signer)) => Ok(Some(signer)),
            Ok(None) => Ok(None),
            Err(e) => bail!(ServerError::DbError)
        }
    }

    async fn insert_signer(&mut self, signer: Signer) -> eyre::Result<Signer> {
        let mut db = self.db_pool.get()?;
        let insert_result = db.transaction(|db| {
            // ensure signer's fid is pre-loaded into user table if not already, do nothing on conflict
            insert_into(users).values(User::empty(signer.fid)).on_conflict_do_nothing().execute(db)?;
            insert_into(signers).values(signer)
                .returning(Signer::as_returning())
                .get_result(db)
        }).map_err(|e| {
            error!("Error inserting into db: {e}");
            ServerError::DbError
        })?;
        Ok(insert_result)
    }
}
