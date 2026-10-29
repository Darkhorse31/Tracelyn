import test from "node:test"
import assert from "node:assert/strict"
import { parseDashboard, STARTERS, isAnomaly, anomalyRate } from "./dashboards.ts"
test("all dashboard starters round trip and use a server variable",()=>{for(const d of STARTERS){assert.deepEqual(parseDashboard(JSON.stringify(d)),d);assert.equal(d.server,"$server")}})
test("dashboard imports reject malformed layouts and duplicate IDs",()=>{for(const patch of [{x:-1},{w:13},{h:0},{kind:"script"}]){const d=structuredClone(STARTERS[0]);Object.assign(d.panels[0],patch);assert.throws(()=>parseDashboard(JSON.stringify(d)))}const d=structuredClone(STARTERS[0]);d.panels[1].id=d.panels[0].id;assert.throws(()=>parseDashboard(JSON.stringify(d)))})
test("baselines warm up and handle zero MAD without flagging noise",()=>{assert.equal(isAnomaly(100,{median:1,mad:0,samples:3}),null);const b={median:100,mad:0,samples:20};assert.equal(isAnomaly(102,b),false);assert.equal(isAnomaly(120,b),true);assert.equal(anomalyRate([100,120],b),.5);assert.equal(anomalyRate([],b),null)})
