import assert from 'node:assert/strict';
import {test} from 'node:test';
import {insertionBefore,wouldMove} from '../src/studio/timeline.js';
import {timeline,reorderTimeline,History} from '../src/studio/model.js';

test('Insertion slots cover left edge, card halves, gaps, and the far end',()=>{
  const cards=[{id:11,center:120},{id:22,center:380},{id:33,center:640}];
  assert.equal(insertionBefore(cards,-20),11);
  assert.equal(insertionBefore(cards,119),11);
  assert.equal(insertionBefore(cards,121),22);
  assert.equal(insertionBefore(cards,250),22);
  assert.equal(insertionBefore(cards,1100),null);
  assert.equal(insertionBefore([],10),null);
});

test('A drop changes only chronology, preserves notes, and is one undo/redo',()=>{
  let story={next_id:40,chapters:[{id:11,name:'Arrival',layers:[{id:1}]},{id:22,name:'Before dawn',layers:[{id:2}]},{id:33,name:'Departure',layers:[]}],timeline:[{chapter_id:11,when:'Day 2',summary:'Arrives'},{chapter_id:22,when:'Day 1',summary:'Flashback'}]};
  const original=structuredClone(story),history=new History(1024*1024);
  assert.ok(wouldMove(timeline(story).map(e=>e.chapter_id),22,11));
  history.push(story);reorderTimeline(story,22,11);
  assert.deepEqual(timeline(story).map(e=>e.chapter_id),[22,11,33]);
  assert.equal(story.timeline[0].summary,'Flashback');
  assert.deepEqual(story.chapters,original.chapters);
  assert.equal(history.undo.length,1);
  story=history.travel(story,'undo');assert.deepEqual(story,original);
  story=history.travel(story,'redo');assert.equal(story.timeline[0].chapter_id,22);
});

test('No-op drops leave the undo history and reading order alone',()=>{
  assert.equal(wouldMove([1,2,3],1,2),false);
  assert.equal(wouldMove([1,2,3],3,null),false);
  assert.equal(wouldMove([1,2,3],2,2),false);
  assert.equal(wouldMove([1,2,3],7,null),false);
  assert.equal(wouldMove([1,2,3],1,null),true);
  assert.equal(wouldMove([1,2,3],3,1),true);
});
