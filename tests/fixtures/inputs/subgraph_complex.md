graph TD
    Client[Web Client]

    subgraph Gateway [API Gateway]
    Auth[Authentication]
    Router[Router]
    end

    subgraph Backend [Backend Services]
    API[REST API]
    Worker[Background Worker]
    end

    subgraph Storage [Data Layer]
    DB[(PostgreSQL)]
    Cache[(Redis)]
    end

    Client --> Auth
    Auth --> Router
    Router --> API
    API --> DB
    API --> Cache
    API -->|async| Worker
    Worker --> DB
