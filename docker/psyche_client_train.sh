#! /bin/bash

set -o errexit

env_file="./config/client/.env"

if [[ ! -f "$env_file" ]]; then
    echo -e "\nEnvironment file does not exist. You must provide one."
    exit 1
fi

source "$env_file"

if [[ ! -f "$WALLET_FILE" ]]; then
    echo -e "\n[!] The file that was set in the WALLET_FILE env variable does not exist."
    exit 1
fi

if [[ "$RPC" == "" ]]; then
   echo -e "\n[!] The RPC env variable was not set."
   exit 1
fi

if [[ "$WS_RPC" == "" ]]; then
   echo -e "\n[!] The WS_RPC env variable was not set."
   exit 1
fi

if [[ "$RUN_ID" == "" ]]; then
   echo -e "\n[!] The RUN_ID env variable was not set."
   exit 1
fi

echo "

 !555555555555Y7^.    .~J55Y7^   ^~7J57       :YY: .~?Y55Y?^  ^5J??JY^               .:!?JY?~.  :~
 .^~5BBP!!7?77?5BGJ: ~PP7!JG#B57?G7?GBBJ      ?#Y~JGG?~!JPBBPJ55~?BB#!             :JPGY?JPBBPJ7G7
    7BB5^JYJJJ~ ^G#G?BB^   .?GBBP:  :PBBY    :G5JB#P:    .~5GJ^  :BBB~ ..:..      :G#5:    ~5BBB7
    ?BBBP~:~!?B? ~BBBBG:     ~PB?:.  .YBBY. ^GY7BBG:        .    .GBBYJJ?JY55?~  .PBB^......:7Y?^~^
    ?BBG:.5?7GB? ~BBBBBP??J5PGBBBBG?.  7BB57GJ JBBY              .GBBY:    .7BB5:!BBBGGGGGGGGGGGB#?
    ?BB5 ~B~.?!:!GG77GBBBGP5Y?!!?BB#7   ~GB#J  ?BBP.             .GBB!       ?BB5?BBB?^^^^::::::?#!
    ?BBB7 ~JJJJYJ7:  .:^:.       JBB7    PBB~  :GBBY.            .GBB7       ~BBG^GBBY.         7G:
    ?BBGP5J7777?J7   ^:          Y#P:    PBB~   7BBB5~        .^ .PBB!       !BB? !BBBP7:       .^
 ?Y J#BP.:~!777!^.  ^BBP?~:.   .?B5:  ..~BB#?^~: ~5GBBP?~^^!J5GB^:GB#?.:::.  J#B7?~^5BB#BPYJ??JJ5P.
 !5JY555J7!:        YG5PGBGP5YJ5Y!   ~YYYJ??77?!   :~7JYYJJJ?7YBJ?JJJJ?JJJ~  YPY7~.  :!?YY5YJ7~:?Y.
                   ?G:  .::^^^:.      .        .~77~.  .      !B:
...................?!.......................Y?~::J?::~?5......!Y:...................................
:::::::::::::::::::..:::::::::::::::::::::::5Y!^:??:^!Y5:::::::.::::::::::::::::::::::::::::::::::::
                                            :  .!JJ!.  :
"

echo "[+] Starting to train in run '${RUN_ID}'..."

docker run --rm -v "$WALLET_FILE":/keys/id.json \
    --gpus all \
    -e NVIDIA_DRIVER_CAPABILITIES=all \
    --name psyche-client \
    psyche-client train \
        --wallet-private-key-path "/keys/id.json" \
        --rpc ${RPC} \
        --ws-rpc ${WS_RPC} \
        --run-id ${RUN_ID} \
        --ticker \
        --logs "console"
