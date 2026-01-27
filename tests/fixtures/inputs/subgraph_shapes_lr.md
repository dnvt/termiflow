graph LR
subgraph SG1 [Decision Flow]
Start((Begin))
Check{Valid?}
Process[Process Data]
Store[(Database)]
Start --> Check
Check --> Process
Process --> Store
end
