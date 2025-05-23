import argparse
import torch
from transformers import AutoModelForCausalLM, AutoTokenizer, pipeline, TextStreamer

DEFAULT_PROMPT = """EDWARD:
I wonder how our princely father 'scaped,
Or whether he be 'scaped away or no
From Clifford's and Northumberland's pursuit:
Had he been ta'en, we should have heard the news;
Had he been slain, we should have heard the news;
Or had he 'scaped, methinks we should have heard
The happy tidings of his good escape.
How fares my brother? why is he so sad?

RICHARD:
I cannot joy, until I be resolved
Where our right valiant father is become.
I saw him in the battle range about;
And watch'd him how he singled Clifford forth.
Methought he bore him in the thickest troop
As doth a lion in a herd of neat;
Or as a bear, encompass'd round with dogs,
Who having pinch'd a few and made them cry,
The rest stand all aloof, and bark at him.
So fared our father with his enemies;
So fled his enemies my warlike father:
Methinks, 'tis prize enough to be his son.
See how the morning opes her golden gates,
And takes her farewell of the glorious sun!
How well resembles it the prime of youth,
Trimm'd like a younker prancing to his love!

EDWARD:
Dazzle mine eyes, or do I see three suns?

RICHARD:
Three glorious suns, each one a perfect sun;
Not separated with the racking clouds,
But sever'd in a pale clear-shining sky.
See, see! they join, embrace, and seem to kiss,
As if they vow'd some league inviolable:
Now are they but one lamp, one light, one sun.
In this the heaven figures some event.

EDWARD:
'Tis wondrous strange, the like yet never heard of.
I think it cites us, brother, to the field,
That we, the sons of brave Plantagenet,
Each one already blazing by our meeds,
Should notwithstanding join our lights together
And over-shine the earth as this the world.
Whate'er it bodes, henceforward will I bear
Upon my target three fair-shining suns.
"""


def main(args):
    model = AutoModelForCausalLM.from_pretrained(
        args.model,
        device_map=args.device,
        torch_dtype=torch.bfloat16,
        trust_remote_code=args.trust_remote_code,
    )
    tokenizer = AutoTokenizer.from_pretrained(args.model)
    streamer = TextStreamer(tokenizer, skip_prompt=True)
    p = pipeline(
        "text-generation",
        model=model,
        tokenizer=tokenizer,
        do_sample=args.temperature and args.temperature > 0,
        temperature=args.temperature if args.temperature else None,
        streamer=streamer,
        max_new_tokens=args.max_tokens,
    )
    p(DEFAULT_PROMPT)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", default="NousResearch/Llama-2-7b-hf")
    parser.add_argument("--temperature", default=0.6, type=float)
    parser.add_argument("--device", default="auto")
    parser.add_argument("--max-tokens", default=1024)
    parser.add_argument("--trust-remote-code", action="store_true")
    main(parser.parse_args())
