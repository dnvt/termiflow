graph TD
    A[Entry] --> B[Loop Start]
    B --> C[Inner]
    C --> D[Check]
    D --> B
    D --> E[Exit]
