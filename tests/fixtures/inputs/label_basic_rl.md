graph RL
    A[Start] -->|validate| B[Process]
    B -->|success| C[Done]
    B -->|error| D[Retry]
