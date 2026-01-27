graph BT
    %% A grid layout with crossing edges - tests crossing minimization
    A1[Node A1] --> B2
    A1 --> B3
    A2[Node A2] --> B1
    A2 --> B3
    A3[Node A3] --> B1
    A3 --> B2

    B1[Node B1] --> C2
    B1 --> C3
    B2[Node B2] --> C1
    B2 --> C3
    B3[Node B3] --> C1
    B3 --> C2

    C1[Node C1]
    C2[Node C2]
    C3[Node C3]
