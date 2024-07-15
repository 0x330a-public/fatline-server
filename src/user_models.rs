use std::time::{SystemTime};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Queryable, Selectable, Insertable, AsChangeset, Debug, Clone)]
#[diesel(table_name=crate::schema::users)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct User {
    pub fid: i64,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub url: Option<String>,
    pub profile_pic: Option<String>
}

impl User {
    pub fn empty(fid: i64) -> Self {
        User {
            fid,
            bio: None,
            url: None,
            profile_pic: None,
            username: None,
            display_name: None
        }
    }
}

#[derive(Queryable, Selectable, Insertable, AsChangeset, Debug, Eq, PartialEq, Hash, Clone)]
#[diesel(table_name=crate::schema::signers)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Signer {
    pub pk: Vec<u8>,
    pub fid: i64,
    pub active: bool
}

#[derive(Queryable, Selectable, Insertable, Associations, Debug, Eq, PartialEq, Hash, Clone)]
#[diesel(table_name=crate::schema::links)]
#[diesel(belongs_to(User, foreign_key=target))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Link {
    pub fid: i64,
    pub target: i64,
    pub timestamp: SystemTime,
}

#[derive(Serialize, Deserialize, Queryable, Selectable, Insertable, Associations, Debug, Clone)]
#[diesel(table_name=crate::schema::notifications)]
#[diesel(belongs_to(User, foreign_key=fid))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Notification {
    pub notification_id: Uuid,
    pub fid: i64,
    pub notification_type: i32,
    pub notification_data: Vec<u8>,
    pub created: SystemTime,
    pub viewed: bool
}
