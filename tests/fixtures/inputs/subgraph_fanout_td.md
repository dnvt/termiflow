graph TD
Router[Request Router]
subgraph SG1 [Handler Group]
H1[Handler 1]
H2[Handler 2]
H3[Handler 3]
end
Router --> H1
Router --> H2
Router --> H3
