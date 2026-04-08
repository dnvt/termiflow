graph BT
A[Source] --> B
subgraph SG [Group]
    B[Target]
    C[Other]
end
D[External] --> C
