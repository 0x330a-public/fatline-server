use eyre::Result;
use axum::async_trait;
use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl, SelectableHelper};
use diesel::connection::LoadConnection;
use fatline_rs::users::{Profile, UserService};
use crate::schema::users::dsl::users;
use crate::schema::users::dsl::fid;
use crate::service::ServiceState;
use crate::user_models::User;

#[async_trait]
pub trait UserRepository {
    async fn get_user_profile(&mut self, fid_q: u64) -> Result<Profile>;
}

impl Into<User> for Profile {
    fn into(self) -> User {
        User {
            fid: self.fid as i64,
            url: self.url,
            username: self.username,
            bio: self.bio,
            display_name: self.display_name,
            profile_pic: self.profile_picture
        }
    }
}

impl Into<Profile> for User {
    fn into(self) -> Profile {
        Profile {
            fid: self.fid as u64,
            url: self.url,
            username: self.username,
            bio: self.bio,
            display_name: self.display_name,
            profile_picture: self.profile_pic
        }
    }
}

#[async_trait]
impl UserRepository for ServiceState {
    async fn get_user_profile(&mut self, fid_q: u64) -> Result<Profile> {
        let mut db = self.db_pool.get()?;
        let existing = users.select(User::as_select()).filter(fid.eq(fid_q as i64)).get_result(&mut db);
        let to_return = match existing {
            Ok(user) => {
                user
            },
            _ => {
                let fetched = self.hub_client.get_user_profile(fid_q).await?;
                let to_store: User = fetched.into();
                diesel::insert_into(users)
                    .values(&to_store)
                    .on_conflict(fid)
                    .do_update()
                    .set(&to_store)
                    .returning(User::as_returning())
                    .get_result(&mut db)?
            }
        };
        Ok(to_return.into())
    }
}
