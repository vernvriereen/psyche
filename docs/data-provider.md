# Data Provider

The data provider is responsible for parsing the training data and creating samples for the clients to use when training a model.

## Overview

The data provider acts as a server that can be accessed via TCP or HTTP by clients to obtain the data they need for training.

When a client starts a round of training, it receives an ID or a range of IDs from the coordinator representing all the batches that will be used for that round. Each batch contains a specific range of the overall data. The client can then call the data provider with the assigned IDs for the run and fetch the corresponding data to begin training.

To better understand how the data is partitioned for each client, refer to the following diagram:

![Data distribution example](images/data-distribution.png)

The number of batches used for training in a run, as well as the indexes of data that each batch contains, can be configured.
