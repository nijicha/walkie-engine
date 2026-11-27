// node tests/version.test.ts: checks the version order the start tool uses for min_engine.
import { strict as assert } from "node:assert"
import { compareVersions as c } from "../scripts/version.ts"

const order = ["0.0.9", "0.1.0-alpha", "0.1.0-alpha.2", "0.1.0-beta", "0.1.0-rc.1", "0.1.0", "0.1.1-alpha", "0.2.0", "1.0.0"]
for (let i = 0; i < order.length; i++)
  for (let j = 0; j < order.length; j++)
    assert.equal(Math.sign(c(order[i], order[j])), Math.sign(i - j), `${order[i]} vs ${order[j]}`)
assert.equal(c("v0.1.0", "0.1.0"), 0)
console.log("version order ok")
