use std::time::{Instant, SystemTime};
use diesel::prelude::*;

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
