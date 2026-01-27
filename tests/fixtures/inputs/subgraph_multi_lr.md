graph LR
subgraph SG1 [Authentication]
A[Login]
B[Validate]
A --> B
end
subgraph SG2 [Processing]
C[Parse]
D[Execute]
C --> D
end
B --> C
