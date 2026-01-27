graph BT
Start[Entry Point]
subgraph SG1 [Core Logic]
A[Process A]
B[Process B]
A --> B
end
End[Exit Point]
Start --> A
B --> End
