graph LR
API[REST API] --> DB[(PostgreSQL)]
API --> Cache[(Redis)]
Cache --> DB
