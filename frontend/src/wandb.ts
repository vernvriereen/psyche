export interface WandBHistoryItem {
  _step: number;

  p2p: {
    nodes: Record<string, { ips: string; bandwidth: number }>;
  };
  train: {
    loss: number;
    confidence: number;
    tokens_per_sec: number;
    total_tokens: number;
  };

  eval: Record<string, number>;

  coordinator: {
    round: number;
    num_clients: number;
    epoch: number;
  };
}

export interface WandBData {
  id: string;
  displayName: string;
  createdAt: string;
  config: {
    total_steps: number;
    rounds_per_epoch: number;
    batches_per_round: number;
    data_indicies_per_batch: number;
  };
  history: WandBHistoryItem[];
  summary: WandBHistoryItem;
}

function slashDotReviver(_key: string, value: JsonValue): JsonValue {
  if (typeof value === "object" && value !== null) {
    const nestedObject: Record<string, JsonValue> = {};

    for (const [key, val] of Object.entries(value)) {
      const parts = key.split(/[/.]/g);

      let current = nestedObject;
      for (let i = 0; i < parts.length; i++) {
        const part = parts[i];

        if (i === parts.length - 1) {
          // Last part, set the value
          current[part] = val;
        } else {
          // Create nested object if it doesn't exist
          current[part] = (current[part] as Record<string, JsonValue>) || {};
          current = current[part] as Record<string, JsonValue>;
        }
      }
    }

    return nestedObject;
  }
  return value;
}

type JsonValue = string | number | boolean | null | JsonValue[] | { [key: string]: JsonValue };

async function gql(query: string, variables: Record<string, JsonValue>) {
  const wandbToken = (() => {
    try {
      return import.meta.env.PUBLIC_WANDB_TOKEN;
    } catch {
      return undefined;
    }
  })();
  const response = await fetch("https://psyche-eight.vercel.app/api/proxy", {
    method: "post",
    body: JSON.stringify({
      query,
      variables,
    }),
    headers: {
      "Content-Type": "application/json",
      ...(wandbToken
        ? {
            Authorization: `Basic ${btoa(`api:${wandbToken}`)}`,
          }
        : {}),
    },
  });

  const json = await response.json();
  return json;
}

export async function getData(
  entity: string,
  project: string,
  name: string,
  samples?: number,
): Promise<WandBData | null> {
  try {
    const _meta = await gql(
      `query Run($project: String!, $entity: String!, $name: String!) {
        project(name: $project, entityName: $entity) {
            run(name: $name) {
                ...RunFragment
            }
        }
    }
    fragment RunFragment on Run {
      id
      name
      displayName
      state
      config
      createdAt
      heartbeatAt
      description
      notes
      systemMetrics
      summaryMetrics
      historyLineCount
      historyKeys
    }`,
      { entity, project, name },
    );
    if (!_meta.data.project) {
      return null;
    }
    const meta = _meta.data.project.run;
    const summary: WandBHistoryItem = JSON.parse(meta.summaryMetrics, slashDotReviver);
    const history = (
      await gql(
        `query RunSampledHistory($project: String!, $entity: String!, $name: String!, $specs: [JSONString!]!) {
            project(name: $project, entityName: $entity) {
                run(name: $name) { sampledHistory(specs: $specs) }
            }
        }`,
        {
          entity,
          project,
          name,
          specs: JSON.stringify({
            samples: samples ?? 500,
            keys: [
              "_step",
              "train/loss",
              "train/confidence",
              "train/tokens_per_sec",
              "train/total_tokens",
              "eval/hellaswag",
              "eval/mmlu",
              "eval/arc_easy",
              "eval/arc_challenge",
              ...Object.keys(summary.p2p.nodes).map((node) => `p2p/nodes.${node}.bandwidth`),
            ],
          }),
        },
      )
    ).data.project.run.sampledHistory[0];
    return {
      id: meta.id,
      createdAt: meta.createdAt,
      displayName: meta.displayName,
      config: JSON.parse(meta.config, slashDotReviver),
      summary,
      history: history.map((line: object) => JSON.parse(JSON.stringify(line), slashDotReviver)),
    };
  } catch (err) {
    console.error(err);
    return null;
  }
}
