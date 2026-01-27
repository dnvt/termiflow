graph RL
subgraph SG1 [Input Stage]
I1[Read]
I2[Parse]
I1 --> I2
end
subgraph SG2 [Transform Stage]
T1[Validate]
T2[Convert]
T1 --> T2
end
subgraph SG3 [Output Stage]
O1[Format]
O2[Write]
O1 --> O2
end
I2 --> T1
T2 --> O1
