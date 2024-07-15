-- Your SQL goes here
create table if not exists notifications
(
    notification_id uuid default gen_random_uuid() primary key,
    fid bigint not null references users(fid) ON DELETE CASCADE NOT NULL,
    notification_type int not null,
    notification_data bytea not null,
    created timestamp not null default now(),
    viewed bool not null default false
);

