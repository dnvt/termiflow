graph LR
    %% Dense diagram with 30+ nodes for scaling tests
    A1[Input 1] --> P1[Process 1]
    A2[Input 2] --> P1
    A3[Input 3] --> P2[Process 2]
    A4[Input 4] --> P2
    A5[Input 5] --> P3[Process 3]
    A6[Input 6] --> P3
    A7[Input 7] --> P4[Process 4]
    A8[Input 8] --> P4

    P1 --> M1[Merge 1]
    P2 --> M1
    P3 --> M2[Merge 2]
    P4 --> M2

    M1 --> F1[Filter 1]
    M2 --> F2[Filter 2]

    F1 --> T1[Transform 1]
    F1 --> T2[Transform 2]
    F2 --> T3[Transform 3]
    F2 --> T4[Transform 4]

    T1 --> O1[Output 1]
    T2 --> O2[Output 2]
    T3 --> O3[Output 3]
    T4 --> O4[Output 4]

    O1 --> END[Done]
    O2 --> END
    O3 --> END
    O4 --> END
