graph BT
Before[Start]
subgraph SG1 [Container]
Solo[Single Node]
end
After[End]
Before --> Solo
Solo --> After
