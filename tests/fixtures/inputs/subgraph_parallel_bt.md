graph BT
subgraph Process [Process]
    A[Start] --> B[Path 1]
    A --> C[Path 2]
    B --> D[End]
    C --> D
end
In[Input] --> A
D --> Out[Output]
