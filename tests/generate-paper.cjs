const assert = require('node:assert/strict')
const { test } = require('node:test')
const { main } = require('../functions/packages/grace/explain-answer/index.js')

test('legacy explain and generated papers use their respective contracts', async t => {
  process.env.OPENAI_API_KEY = 'test'
  const paper = { format_version: 1, title: 'Travel', questions: [{ id: 'q1', type: 'choice' }] }
  let content = JSON.stringify(paper)
  t.mock.method(globalThis, 'fetch', async () => Response.json({
    choices: [{ finish_reason: 'stop', message: { content } }]
  }))
  const event = { http: { method: 'POST' }, action: 'generate_paper', topic: 'Travel', count: 1 }
  assert.deepEqual((await main(event)).body, paper)
  assert.equal((await main({ ...event, count: 2 })).statusCode, 502)
  assert.equal((await main({ ...event, count: 11 })).statusCode, 400)
  for (const types of [['unknown'], ['choice', 'choice'], ['choice', 'sentence'], null]) {
    assert.equal((await main({ ...event, question_types: types })).statusCode, 400)
  }
  assert.equal((await main({ ...event, question_types: ['translation'] })).statusCode, 502)
  const ten = { ...paper, questions: Array.from({ length: 10 }, (_, i) => ({
    id: 'q' + i, type: i % 2 ? 'sentence' : 'choice'
  })) }
  content = JSON.stringify(ten)
  assert.deepEqual((await main({ ...event, count: 10, question_types: ['sentence', 'choice'] })).body, ten)
  assert.equal((await main({ ...event, count: 10, question_types: ['sentence', 'choice', 'translation'] })).statusCode, 502)
  content = 'Good answer.'
  assert.deepEqual((await main({ http: event.http, prompt: 'Hello?', submitted_answer: 'Bonjour',
    reference_answer: 'Bonjour', is_correct: true })).body, { explanation: content })
})
