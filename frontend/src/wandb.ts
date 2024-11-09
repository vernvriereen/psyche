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

function slashReviver(_key: string, value: JsonValue) {
	if (typeof value === "object" && value !== null) {
		const nestedObject: Record<string, JsonValue> = {};
		for (const [k, v] of Object.entries(value)) {
			const [parentKey, childKey] = k.split("/");
			if (childKey) {
				nestedObject[parentKey] =
					(nestedObject[parentKey] as Record<string, JsonValue>) || {};
				nestedObject[parentKey][childKey] = v;
			} else {
				nestedObject[k] = v;
			}
		}
		return nestedObject;
	}
	return value;
}

type JsonValue =
	| string
	| number
	| boolean
	| null
	| JsonValue[]
	| { [key: string]: JsonValue };

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
    }`,
			{ entity, project, name },
		);
		if (!_meta.data.project) {
			return null;
		}
		const meta = _meta.data.project.run;
		const history = (
			await gql(
				`query RunFullHistory($project: String!, $entity: String!, $name: String!, $samples: Int) {
      project(name: $project, entityName: $entity) {
        run(name: $name) {
          history(samples: $samples)
        }
      }
    }`,
				{
					entity,
					project,
					name,
					...(samples !== undefined ? { samples } : {}),
				},
			)
		).data.project.run;

		return {
			id: meta.id,
			createdAt: meta.createdAt,
			displayName: meta.displayName,
			config: JSON.parse(meta.config, slashReviver),
			summary: JSON.parse(meta.summaryMetrics, slashReviver),
			history: history.history.map((line: string) =>
				JSON.parse(line, slashReviver),
			),
		};
	} catch (err) {
		console.error(err);
		return null;
	}
}
