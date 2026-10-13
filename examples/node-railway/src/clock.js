import cron from 'node-cron';
import { pool } from './db.js';
import { jobs } from './queue.js';

const cleanup = () => pool.query('delete from tokens where expires_at < now()');
const report = () => jobs.add('report', {});
const digest = () => jobs.add('digest', {});

cron.schedule('0 * * * *', cleanup);
cron.schedule('0 2 * * *', report);
cron.schedule('0 8 * * 1', digest);
