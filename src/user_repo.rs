use std::collections::BTreeSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::async_trait;
use chrono::{DateTime, NaiveDateTime};
use diesel::{BoolExpressionMethods, Connection, EqAll, ExpressionMethods, Insertable, JoinOnDsl, QueryDsl, QuerySource, RunQueryDsl, Selectable, SelectableHelper, Table};
use diesel::associations::HasTable;
use diesel::connection::SimpleConnection;
use diesel::result::Error;
use diesel::result::Error::DatabaseError;
use eyre::{bail, OptionExt, Result};
use fatline_rs::action::LinkAction;
use fatline_rs::proto::{FidRequest, LinksByFidRequest, LinksByTargetRequest};
use fatline_rs::proto::links_by_target_request::Target;
use fatline_rs::users::{Profile, UserService};
use fatline_rs::utils::link_from_message;
use r2d2_postgres::postgres::fallible_iterator::FallibleIterator;
use tracing::debug;
use crate::schema;

use crate::schema::links::dsl::fid as l_fid;
use crate::schema::links::dsl::links;
use crate::schema::links::dsl::target;
use crate::schema::notifications::dsl::notifications;
use crate::schema::users::dsl::fid as u_fid;
use crate::schema::users::dsl::users;
use crate::service::ServiceState;
use crate::user_models::{Link, Notification, User};

#[derive(Debug, Copy, Clone)]
pub enum FollowDirection {
    Following,
    FollowedBy
}

#[async_trait]
pub trait UserRepository {
    async fn get_user_profile(&self, fid_q: u64, force_fetch: bool) -> Result<Profile>;
    async fn fetch_and_store_profile(&self, fid_q: u64) -> Result<Profile>;
    async fn get_profile_links(&self, fid_q: u64, force_fetch: bool, direction: FollowDirection) -> Result<Vec<Profile>>;
    async fn get_user_notifications(&self, fid_q: u64) -> Result<Vec<Notification>>;
    async fn fetch_and_store_links(&self, fid_q: u64, direction: FollowDirection) -> Result<Vec<Profile>>;
    async fn fetch_user_latest_notification_type(&self, fid_q: u64) -> Result<i32>;
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
    async fn get_user_profile(&self, fid_q: u64, force_fetch: bool) -> Result<Profile> {
        if force_fetch {
            return self.fetch_and_store_profile(fid_q).await;
        }
        let existing = {
            let mut db = self.db_pool.get()?;
            users.select(User::as_select()).filter(u_fid.eq(fid_q as i64)).get_result(&mut db)
        };
        let to_return = match existing {
            Ok(user) => {
                user
            },
            _ => {
                self.fetch_and_store_profile(fid_q).await?.into()
            }
        };
        Ok(to_return.into())
    }

    async fn fetch_and_store_profile(&self, fid_q: u64) -> Result<Profile> {
        let mut db = self.db_pool.get()?;
        let fetched = {
            let mut hub_client = self.hub_client.lock().await.clone();
            hub_client.get_user_profile(fid_q).await?
        };
        let to_store: User = fetched.into();
        let insert = diesel::insert_into(users)
            .values(&to_store)
            .on_conflict(u_fid)
            .do_update()
            .set(&to_store)
            .returning(User::as_returning())
            .get_result(&mut db)?;
        Ok(insert.into())
    }

    async fn get_profile_links(&self, fid_q: u64, force_fetch: bool, direction: FollowDirection) -> Result<Vec<Profile>> {
        if force_fetch {
            return self.fetch_and_store_links(fid_q, direction).await;
        }
        let existing = {
            let mut db = self.db_pool.get()?;
            match direction {
                FollowDirection::FollowedBy =>
                    users.select(User::as_select())
                        .filter(
                            u_fid.eq_any(
                                links.select(l_fid).filter(target.eq(fid_q as i64))
                            )
                        )
                        .get_results(&mut db),
                FollowDirection::Following =>
                    users.select(User::as_select())
                        .filter(
                            u_fid.eq_any(
                                links.select(target).filter(l_fid.eq(fid_q as i64))
                            )
                        )
                        .get_results(&mut db)
            }
        };

        if let Ok(existing_users) = existing {
            return Ok(existing_users.iter().cloned().map(|u| u.into()).collect())
        };

        Ok(self.fetch_and_store_links(fid_q, direction).await?)
    }

    async fn fetch_and_store_links(&self, fid_q: u64, direction: FollowDirection) -> Result<Vec<Profile>> {
        let mut db = self.db_pool.get()?;
        let mut hub_client = {
            let locked = self.hub_client.lock().await;
            locked.clone()
        };

        let mut to_add = BTreeSet::new();
        let mut adds = vec![];
        let mut deletes = vec![];

        let fetched = match direction {
            FollowDirection::Following => hub_client.get_links_by_fid(LinksByFidRequest {
                 fid: fid_q,
                 page_token: None,
                 reverse: None,
                 page_size: None,
                 link_type: Some("follow".to_string()),
             }).await?.into_inner(),
            FollowDirection::FollowedBy => hub_client.get_links_by_target(LinksByTargetRequest {
                link_type: Some("follow".to_string()),
                page_size: None,
                reverse: None,
                page_token: None,
                target: Some(Target::TargetFid(fid_q))
            }).await?.into_inner()
        }.messages.iter().cloned().filter_map(link_from_message)
            .flatten()
            .collect::<Vec<_>>();

        for action in fetched {
            match action {
                LinkAction::AddFollow(follow) => {
                    to_add.insert(follow.source_fid as i64);
                    to_add.insert(follow.target_fid as i64);
                    adds.push(follow);
                }
                LinkAction::RemoveFollow(remove) => {
                    deletes.push((remove.source_fid, remove.target_fid));
                }
            }
        }
        
        db.transaction::<_,eyre::Error,_>(|db| {

            // create our user to ensure key constraints
            diesel::insert_into(users::table())
                .values(to_add.iter().copied().map(|f| User::empty(f)).collect::<Vec<_>>())
                .on_conflict_do_nothing()
                .execute(db)?;

            diesel::insert_into(links::table())
                .values(adds.iter().map(|info|{
                    let timestamp = fatline_rs::utils::fc_timestamp_to_unix(info.timestamp).unwrap_or_default();
                    let date = DateTime::from_timestamp(timestamp as i64, 0).unwrap_or_default();
                    Link {
                        fid: info.source_fid as i64,
                        target: info.target_fid as i64,
                        timestamp: SystemTime::from(date),
                    }
                }).collect::<Vec<_>>())
                .on_conflict_do_nothing()
                .execute(db)?;

            for (s,t) in deletes {
                diesel::delete(links.filter(
                    l_fid.eq(s as i64)
                        .and(target.eq(t as i64))
                )).execute(db)?;
            }

            Ok(())
        })?;

        Ok(match direction {
            FollowDirection::FollowedBy =>
                users.select(User::as_select())
                    .filter(
                        u_fid.eq_any(
                            links.select(l_fid).filter(target.eq(fid_q as i64))
                        )
                    )
                    .load(&mut db),
            FollowDirection::Following =>
                users.select(User::as_select())
                    .filter(
                        u_fid.eq_any(
                            links.select(target).filter(l_fid.eq(fid_q as i64))
                        )
                    )
                    .load(&mut db)
        }.map(|vec| vec.iter().cloned().map(|u|u.into()).collect())?)
    }

    async fn get_user_notifications(&self, fid_q: u64) -> Result<Vec<Notification>> {
        let mut db = self.db_pool.get()?;
        let user_notifications = notifications.select(Notification::as_select())
            .filter(schema::notifications::fid.eq(fid_q as i64))
            .get_results(&mut db)?;
        Ok(user_notifications)
    }

    async fn fetch_user_latest_notification_type(&self, fid_q: u64) -> Result<i32> {
        let mut db = self.db_pool.get()?;
        let latest_type = notifications.select(schema::notifications::notification_type)
            .filter(schema::notifications::fid.eq(fid_q as i64))
            .get_result(&mut db)?;
        Ok(latest_type)
    }

}
