graph TD
    X[External]
    subgraph Backend
    A[API]
    B[Cache]
    end
    X --> A
    A --> B
