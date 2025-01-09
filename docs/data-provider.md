# Data Provider

The data provider is responsible to parse the training data and create samples for the clients to start training a model.

## Overview

The data provider acts a server that can be accesses via tcp or http from the clients to obtain the data they will need to train in the run. The data can be tokenized or just works as a raw data.

When a client start a round of training they will receive an ID representing a batch  that contains a specific range of content of the whole data. The client then can called the data provider with the data that was assigned for this run by the coordinator and fetch them to keep training.

To have a better knowledge about how the data is partitioned for every client let's look at the following diagram.

TODO: CHECK THIS
![Data distribution](images/data-distribution.png)
