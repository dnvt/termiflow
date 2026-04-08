graph LR
subgraph Outer [Outer]
    subgraph Inner [Inner]
        subgraph Deep [Deep]
            A[Node]
        end
    end
end
B[In] --> A
A --> C[Out]
