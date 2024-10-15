export interface WandBHistoryItem {
  _step: number;
  "train/loss": number;
  "train/certainty": number;
  "train/tokens_per_sec": number;
  "train/total_tokens": number;

  "eval/mmlu_pro"?: number;
  "eval/hellaswag"?: number;
  "eval/arc_easy"?: number;
  "eval/arc_challenge"?: number;

  "coordinator/round": number;
  "coordinator/num_clients": number;
  "coordinator/epoch": number;
}

export async function getData(
  entity: string,
  project: string,
  name: string,
  samples?: number
): Promise<WandBHistoryItem[]> {
  const data = JSON.stringify({
    query: `query RunFullHistory($project: String!, $entity: String!, $name: String!, $samples: Int) {
      project(name: $project, entityName: $entity) {
        run(name: $name) {
          history(samples: $samples)
        }
      }
    }`,
    variables: {
      entity,
      project,
      name,
      samples,
    },
  });

  const response = await fetch("https://api.wandb.ai/graphql", {
    method: "post",
    body: data,
    headers: {
      "Content-Type": "application/json",
      ...(import.meta.env.PUBLIC_WANDB_TOKEN
        ? {
            Authorization: `Basic ${btoa(
              `api:${import.meta.env.PUBLIC_WANDB_TOKEN}`
            )}`,
          }
        : {}),
    },
  });

  const json = await response.json();
  return json.data.project.run.history.map((line: string) => JSON.parse(line));
}
