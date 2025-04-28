# Welcome to Psyche

<p align="center" width="100%">
    <img src="./psyche.jpg">
</p>

Psyche is a system that enables distributed training of transformer-based AI models over the internet, aiming to foster collaboration between untrusted parties to create state-of-the-art machine learning models.
It leverages a peer-to-peer distributed network for communication and data sharing.

This documentation provides a comprehensive guide to understanding, using, and developing with Psyche, whether you're an end-user looking to participate in a training run, a developer interested in contributing to the project, or just curious about how it all works.

## Introduction

<p align="center" width="100%">
    <a href="https://www.youtube.com/watch?v=XMWI3nDk48c">
        <img src="./psyche_youtube.png">
    </a>
</p>

## How does it work?

At its core, Psyche is a protocol that coordinates multiple independent clients to train a single machine learning model together. Rather than running on a centralized server farm with high-speed interconnects between every accelerator (GPUs, usually), Psyche distributes the training workload across many independent computers, each contributing a small piece to the overall training process.

Psyche is built to maintain training integrity without requiring participants to trust each other. Through a combination of consensus mechanisms, game theory, and careful protocol design, Psyche will ensure that the trained model remains coherent and consistent despite being trained across disparate machines.

## Decentralized training flow

```mermaid
flowchart TD
 subgraph sg_solana["Solana"]
    direction TB
        CoordinatorState["Coordinator Program State <br> (Run State, Epoch,<br>Round, Clients)"]
  end
 subgraph sg_distro["DisTrO Optimizer"]
    direction TB
        MomentumUpdate["Update Local Momentum <br> m<sub>t</sub> = βm<sub>t-1</sub> + g<sub>t</sub>"]
        DCTExtract["Extract Fast Components <br> (q<sub>t</sub>) (DCT + TopK)"]
        CompressedUpdate["Compressed Local q<sub>t</sub> <br> (Indices + Amplitudes)"]
        MomentumResidual["Update Local<br>Momentum Residual<br> m<sub>t+1</sub> = m<sub>t</sub> - q<sub>t</sub>"]
  end
 subgraph sg_loop["Local Training"]
    direction TB
        LocalWeights["Model Weights (x<sub>t</sub>)"]
        ApplyAggregatedUpdate["Apply Aggregated Update <br> x<sub>t</sub> = x<sub>t-1</sub> - η Q<sub>t-1</sub>"]
        ReceiveDecode["Receive &amp;<br>Decode/Aggregate <br> Compressed q<sub>t-1</sub><br> from Peers"]
        ForwardBackward["Forward/Backward Pass <br> (Use x<sub>t</sub>, <br>Compute Gradient g<sub>t</sub>)"]
        FetchData["Fetch Assigned Data <br> (Batch<sub>t</sub>)"]
        Gradient["Local Gradient (g<sub>t</sub>)"]
        sg_distro
        P2PNetworkInterface["P2P Network Interface"]
  end
 subgraph sg_client["Client"]
    direction TB
        ClientSM["Client State Machine <br> (Warmup, Train,<br>Witness, Cooldown)"]
        sg_loop
  end
 subgraph sg_p2p["P2P Gossip & Blob Transfer"]
    direction TB
        ClientNode2("Client Node 2")
        ClientNode3("Client Node 3")
        ClientNodeN("Client Node N")
  end
    DataProvider["Data Provider <br> (Local File/HTTP/etc.)"]
    ClientSM -- Manages --> sg_loop
    ClientSM -- Receives State Updates --- CoordinatorState
    ApplyAggregatedUpdate --> LocalWeights
    ReceiveDecode -- "Aggregated Q<sub>t-1</sub>" --> ApplyAggregatedUpdate
    LocalWeights -- Used By --> ForwardBackward
    FetchData -- Provides Data --> ForwardBackward
    ForwardBackward -- Produces Gradient --> Gradient
    Gradient -- Updates --> MomentumUpdate
    MomentumUpdate --> DCTExtract
    DCTExtract -- Produces --> CompressedUpdate
    DCTExtract -- Updates --> MomentumResidual
    CompressedUpdate -- Broadcasts Local Compressed Update --> P2PNetworkInterface
    P2PNetworkInterface -- Receives Compressed Updates --> ReceiveDecode
    DataProvider -- Provides Data --> FetchData
    P2PNetworkInterface <-- Send/Receive Updates -------> sg_p2p
    ClientNode2 <-- Transfer Data Off-chain --> ClientNode3 & ClientNodeN
    ClientNode3 <-- Transfer Data Off-chain --> ClientNodeN
    CoordinatorState -- Assigns Data/Committee --> ClientSM
    ClientSM -- "Submits Transactions (e.g., Join, Tick, Witness)" --> CoordinatorState
```
