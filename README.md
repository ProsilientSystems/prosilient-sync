# Prosilient Sync

Prosilient Sync is a high-performance synchronization server built in Rust, utilizing Tokio and Axum. It facilitates real-time data exchange between devices and a central cloud instance using Protocol Buffers and WebSockets.

## Architecture

- **Engine:** Rust (Axum, Tokio)
- **Protocol:** Protocol Buffers (via `prosilient-geri-protobuf`)
- **Persistence:** PostgreSQL
- **Security:** Ed25519 signature verification (sodiumoxide)

## Deployment

### Prerequisites

- PostgreSQL 14+
- Docker and Docker Compose (optional for containerized deployment)

### Database Initialization

This application does not perform automatic schema migrations. The database must be initialized manually prior to application startup. 

1. Create a PostgreSQL database.
2. Execute the provided `init.sql` script to establish the required schema:

```bash
psql -h <host> -U <user> -d <database> -f init.sql
```

### Configuration

Environment variables are used for configuration. Refer to `.env.example` for the required keys.

- `DATABASE_URL`: PostgreSQL connection string.
- `PORT`: Server listening port (default: 8080).
- `RUST_LOG`: Logging verbosity levels.

### Execution

#### Native
```bash
cargo run --release
```

#### Docker Compose
```bash
docker-compose up --build
```

## License

This project is licensed under the Mozilla Public License 2.0 (MPL-2.0). See [LICENSE](LICENSE) for details.