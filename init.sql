BEGIN;

SET client_encoding = 'LATIN1';

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE authorized_devices(
        machine_id text,
        public_key bytea,
        is_online timestamptz,
        authorized_on timestamptz,
        PRIMARY KEY(machine_id)
);

CREATE TYPE message_types AS ENUM ('Connect', 'Biosensor', 'Scale', 'Survey', 'Event');

CREATE TABLE telemetry(
        telemetry_id uuid DEFAULT uuid_generate_v4(),
        machine_id text,
        server_time_stamp timestamptz,
        machine_time_stamp timestamptz,
        message_type message_types,
        PRIMARY KEY(telemetry_id)
);

CREATE TYPE event_types AS ENUM (
    'survey_initiated',
    'survey_shown',
    'survey_started',
    'survey_completed',
    'survey_expired',
    'weight_initiated',
    'weight_shown',
    'weight_completed',
    'weight_expired',
    'followup_initiated',
    'followup_shown',
    'followup_started',
    'followup_completed',
    'followup_expired',
    'protocol_initiated',
    'protocol_shown',
    'protocol_started',
    'protocol_completed',
    'protocol_expired',
    'biosensor_low_battery',
    'biosensor_chargning',
    'biosensor_sync_delayed',
    'machine_switched_on',
    'machine_time_sync_error',
    'cloud_sync_error',
    'reset_protocol',
    'cloud_disconnect');


CREATE TABLE events(
        event_id uuid DEFAULT uuid_generate_v4(),
        telemetry_id uuid,
        event event_types NOT NULL,
        -- this should be client_event_identifier
        event_identifier uuid NOT NULL,
        machine_event_time timestamptz,
        PRIMARY KEY (event_id),
        FOREIGN KEY (telemetry_id) REFERENCES telemetry(telemetry_id)
);


CREATE TABLE biosensor(
        telemetry_id uuid,
        -- this should be event id of protocol started.
        event_id uuid NOT NULL,
        x_axis float8,
        y_axis float8,
        z_axis float8,
        created_on timestamptz,
        FOREIGN KEY (telemetry_id) REFERENCES telemetry(telemetry_id),
        FOREIGN KEY (event_id) REFERENCES events(event_id)
);


CREATE TABLE scale(
        telemetry_id uuid,
        -- this should be event id of completed.
        event_id uuid NOT NULL,
        weight float8,
        impedance float8,
        unit text,
        created_on timestamptz,
        PRIMARY KEY (telemetry_id),
        FOREIGN KEY (telemetry_id) REFERENCES telemetry(telemetry_id),
        FOREIGN KEY (event_id) REFERENCES events(event_id)
);

-- shown
-- SURVEY_INITIATED, SURVEY_UUID, SURVEY_INITIATED_TIME
-- completed
-- SURVEY_COMPLETED, SURVEY_UUID, SURVEY_COMPLETED_TIME
-- expired
-- SURVEY_EXPIRED, SURVEY_UUID, SURVEY_EXPIRY_TIME

CREATE TYPE survey_type AS ENUM (
    'weekley_survey',
    'follow_up',
    'self_rated'
);

CREATE TABLE survey(
        telemetry_id uuid,
        -- this should be event id of completed.
        event_id uuid NOT NULL,
        survey_id text,
        survey_type survey_type,
        answers json,
        -- symptoms text[],
        FOREIGN KEY (telemetry_id) REFERENCES telemetry(telemetry_id),
        FOREIGN KEY (event_id) REFERENCES events(event_id)
);

CREATE TABLE generations(
        -- zip file nano id
        gen_id text,
        params JSON,
        requested_payload JSON,
        created_on timestamptz,
        gen_status JSON,
        PRIMARY KEY (gen_id)
);

SET timezone = 'America/Chicago';

COMMIT;
