# Data Provider  

The data provider is responsible for parsing the training data and creating
samples for the clients to use when training a model.  

## Overview  

The data provider acts as a server that can be accessed via TCP by clients to
obtain the data they need for training.  

When a client starts a round of training, it receives an ID or a range of IDs
from the coordinator, representing all the batches that will be used for that
round. Each batch contains a specific range of the overall data. The client can
then call the data provider with the assigned IDs for the run and fetch the
corresponding data to begin training.  

To better understand how the data is partitioned for each client, refer to the
following diagram:  

![Data distribution example](images/data-distribution.png)  

The number of batches used for training in a run, as well as the indexes of
data that each batch contains, can be configured.  

## Deep Dive  

When loading a model, there are two configuration files that must be declared
for the coordinator to start working: `data.toml` and `state.toml`. Examples of
these files can be found in the `config` folder.  

The `data.toml` file contains configuration for the data itself in case of
running local training, such as the data's location, token size, sequence
length, and a seed to shuffle the data if needed. The `state.toml` file
contains configuration for the entire run. Of particular interest is the
`[model.LLM.data_location]` section, which defines whether the data will be
hosted on a server or in a local folder. If it is a server, the IP must be
specified, as it is where the clients will connect.  

The `init_run` function initializes the data provider using the configuration
and creates a `DataFetcher`, the structure responsible for managing the data
fetching process. The data fetcher is part of the `TrainingStepMetadata`, which
holds the internal data for the training step within the `StepStateMachine`,
along with other metadata—one for each step.  

Once the data provider is created and included in the state machine, it will be
used at the start of the epoch and during every training step. The client
monitors changes in the coordinator's state, and upon detecting a step
transition, it calls the `apply_state` function for the `RunManager`. This, in
turn, calls the `apply_state` function for the `StepStateMachine`. If the state
indicates that a training round is starting, the `start` function for the
`TrainingStepMetadata` is invoked.  

The `start` function initiates the actual training process on the client side.
Its first task is to fetch the data required for training using the
`assign_data_for_state` function. This function determines the number of
batches for the round and the indices of data within each batch. The client is
then assigned an interval of batch IDs, called `data_assignments`, which it
fetches from the data provider using the `fetch_data` function of the
`DataFetcher`.  

The `fetch_data` function parses the batch IDs using the data indices per batch
to calculate the actual intervals of data to use. It creates a channel to send
and receive batches. Once the data intervals are calculated, the client calls
the `get_samples` function on the data provider to retrieve the raw data for
those IDs. This process repeats in a loop until all batch IDs are requested and
sent through the channel.  

On the other end, the receiver is used in the `train` function. It continuously
receives data from the channel and uses it for training until all data is
consumed.  
