graph LR
subgraph SG1 [Data Sources]
S1[Source 1]
S2[Source 2]
S3[Source 3]
end
Merge[Aggregator]
S1 --> Merge
S2 --> Merge
S3 --> Merge
