graph TD
    Gateway[API Gateway] --> Auth[Auth Service]
    Gateway --> API[Main API]
    Auth --> DB[(Database)]
    API --> DB
    API --> Cache[Redis Cache]
