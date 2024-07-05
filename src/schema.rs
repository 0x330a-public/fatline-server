// @generated automatically by Diesel CLI.

diesel::table! {
    links (fid, target) {
        fid -> Int8,
        target -> Int8,
        timestamp -> Timestamp,
    }
}

diesel::table! {
    signers (pk) {
        pk -> Bytea,
        fid -> Int8,
        active -> Bool,
    }
}

diesel::table! {
    users (fid) {
        fid -> Int8,
        username -> Nullable<Text>,
        display_name -> Nullable<Text>,
        bio -> Nullable<Text>,
        url -> Nullable<Text>,
        profile_pic -> Nullable<Text>,
    }
}

diesel::joinable!(signers -> users (fid));

diesel::allow_tables_to_appear_in_same_query!(
    links,
    signers,
    users,
);
