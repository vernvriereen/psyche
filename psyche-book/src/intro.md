# Overview

Psyche is a system that empowers strangers to collaboratively train a machine learning model in a decentralized and trustless manner.

Read the Psyche annoucement [here](https://nousresearch.com/nous-psyche).

The Psyche code is available on GitHub at [PsycheFoundation/psyche](https://github.com/PsycheFoundation/psyche).

The system is composed of three main actors:

- **Coordinator**: Serves as a source of truth for global state available to all clients in a given training run. Each run has one coordinator that oversees the entire process. The coordinator is implemented as a program running on the Solana Blockchain.
- **Client**: A user participating in a training run. Clients receive the model to be trained and a specific dataset for that run. They send information to the coordinator to progress the training run and use a peer-to-peer network to share their results at each training step with other clients.
- **Data Provider**: An optional server that stores the data to be used for model training, to be serverd to clients. A run could use the data provider, an HTTP location for data, or make clients bring their own copy of the dataset.

```mermaid
flowchart TB
    subgraph run id: test_model_2
        direction TB
        subgraph Solana
            C(("Coordinator"))
        end
        C <--> C1(("Client")) & C2(("Client")) & C3(("Client"))
        C1 <-.-> C2
        C3 <-.-> C2 & C1
        DT["Data hosted on HTTP"] --> C1 & C2 & C3
    end
    subgraph run id: test_model_1
        direction TB
        subgraph Solana2["Solana"]
            CC(("Coordinator"))
        end
        CC <--> C11(("Client")) & C22(("Client")) & C33(("Client"))
        C11 <-.-> C22
        C33 <-.-> C22 & C11
        DTT["Data server"] --> C11 & C22 & C33
    end
```

# What does the training process look like?

The training process for a given model is divided into small steps that incrementally train the model in a coordinated manner. A training run is divided into **epochs**, where clients can join and leave the run, and **epochs** are further divided into **steps**, where the model is incrementally trained.

During a training run, clients primarily perform three tasks:

- **Training**: Train the model using an assigned subset of the data.
- **Witnessing**: Verify the liveness and correctness of other participants.
- **Verifying**: Recompute and compare results to identify and mitigate malicious participants.

## Waiting for Clients & Warmup

At the start of an **epoch**, all clients have a window of time to join the run by requesting to be added by coordinator, and then connecting to the other participating clients.

Once a minimum threshold of clients has been met, the run will transition to the _Warmup_ phase and begin a countdown to allow connected clients to update their copy of the model, at which point it will enter the _Training_ phase.

## Training

At the beginning of an **epoch**, after the _Warmup_ phase ends, clients are assigned specific tasks that require them to train the model on a portion of the data.

The coordinator contains information that uniquely assigns pieces of training data to clients based on the current **round**.

If clients have already been training (i.e., it is not the first round of the epoch), they will apply the results from the previous round, then retrieve the data sample they need for the current round.

After completing the training on their assigned data, each client emits a p2p broadcast to all other clients containing their training results and a cryptographic commitment that binds them to those results.

As the training results are recieved from other clients, they are downloaded to be later incorporated into the current model.

## Witnessing

At the start of each round, one or more clients are randomly selected as witnesses. The number of witnesses can be configured. Witnesses train the model as usual, but also build bloom filters that track which nodes they have recieved training results from, signifying that they are actively participating and providing valid results.

These bloom filters are sent to the coordinator, which then combines them into a provable consensus of which results to apply to the model.

Once a witness quorum is reached, the coordinator advances to the _Training_ phase to allow all clients a brief window to download every training result.

Once the _Witness_ phase concludes, the coordinator returns to the _Training_ phase. Clients are assigned new data, and the process repeats. After a predefined number of rounds, a _Cooldown_ round occurs, marking the end of an **epoch**.


## The witness/train loop visualized
Here's a high-level overview of the process. Additional details exist, but this captures the overall flow:

```mermaid
sequenceDiagram
    participant Client1
    participant Client2
    participant Coordinator
    participant DataServer
    Client1->>DataServer: get_data
    Client2->>DataServer: get_data
    Coordinator->>Client2: witness
    Note over Client1: Train
    Note over Client2: Train
    Client1->>Client2: Send results
    Client2->>Client1: Send results
    Note over Client1: Download results
    Note over Client2: Download results
    Client2->>Coordinator: Send witness
    Note over Coordinator: Quorum reached
    Note over Coordinator: Starting Witness phase
```