"""Regression tests for the standalone monitoring script."""

import tempfile
import unittest
from pathlib import Path

import numpy as np
import pandas as pd

from unified_model_monitor import ModelMonitor, select_representative_epochs


class ModelMonitorTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.graph = self.root / 'graph'
        self.graph.mkdir()
        self.monitor = ModelMonitor(self.root)

    def write_new(self, name, epochs=(0, 1, 5, 10, 25, 50), losses=None):
        directory = self.graph / name
        directory.mkdir()
        if losses is None:
            losses = [6 - i for i in range(len(epochs))]
        pd.DataFrame({'epoch': epochs, 'avg_loss': losses}).to_csv(
            directory / 'epoch-loss.txt', index=False)
        pd.DataFrame({'epoch': epochs, 'batch_num': [1] * len(epochs),
                      'loss': losses}).to_csv(directory / 'batch-loss.txt', index=False)

    def write_flat(self, name):
        (self.graph / f'epoch-loss-{name}.txt').write_text('0 2\n2 1\n')
        (self.graph / f'batch-loss-{name}.txt').write_text('0   1  2\n2  1  1\n')

    def test_discovery_mixed_formats_and_duplicate_priority(self):
        self.write_new('new')
        self.write_flat('old')
        self.write_flat('new')
        found = self.monitor.discover_experiments()
        self.assertEqual({item['id'] for item in found}, {'new', 'old'})
        self.assertEqual(next(item for item in found if item['id'] == 'new')['format'], 'directory')
        epoch, batch = self.monitor.load_experiment_data(
            next(item for item in found if item['id'] == 'old'))
        self.assertEqual((len(epoch), len(batch)), (2, 2))

    def test_invalid_inputs_do_not_stop_other_experiments(self):
        self.write_new('good')
        self.write_new('empty')
        (self.graph / 'empty' / 'epoch-loss.txt').write_text('epoch,avg_loss\n')
        self.write_new('missing')
        (self.graph / 'missing' / 'epoch-loss.txt').write_text('epoch,other\n0,2\n1,1\n')
        self.write_new('one', epochs=(1,), losses=(1,))
        self.monitor.run()
        self.assertTrue((self.root / 'model_result_monitoring/good/experiment_report_good.md').exists())
        for name in ('empty', 'missing', 'one'):
            self.assertFalse((self.root / 'model_result_monitoring' / name).exists())

    def test_discontinuous_epochs_nan_and_zero_cv(self):
        self.write_new('nan', losses=(5, 4, np.nan, 2, 1, 0))
        experiment = self.monitor.discover_experiments()[0]
        epoch, batch = self.monitor.load_experiment_data(experiment)
        self.assertEqual(epoch['epoch'].tolist(), [0, 1, 10, 25, 50])
        self.assertEqual(select_representative_epochs(batch), [0, 1, 10, 25, 50])
        slope = self.monitor.calculate_loss_slope_analysis(epoch)
        self.assertEqual(len(slope['dloss_depoch']), 5)
        stats = self.monitor.calculate_epoch_statistics(
            pd.DataFrame({'epoch': [0, 0, 1, 1], 'loss': [0, 0, 1, 2]}))
        self.assertTrue(np.isnan(stats.loc[stats.epoch == 0, 'cv'].iloc[0]))
        self.assertFalse(np.isinf(stats['cv']).any())

    def test_comparison_ranking_matches_displayed_improvement(self):
        self.write_new('high', epochs=(0, 1), losses=(10, 1))
        self.write_new('low', epochs=(0, 1), losses=(2, 1))
        self.monitor.run()
        report = (self.root / 'model_result_monitoring/comparison_report.md').read_text()
        self.assertIn('Loss 개선량 기준', report)
        self.assertIn('| 1 | high |', report)
        self.assertIn('Loss 개선량이 가장 큰 실험', report)
        self.assertNotIn('기울기 소실', report)
        comparison = pd.read_csv(self.root / 'model_result_monitoring/consolidated_csv/experiments_comparison.csv')
        self.assertEqual(set(comparison['experiment_id']), {'high', 'low'})

    def test_excessive_nan_and_nonnumeric_values_are_rejected(self):
        self.write_new('bad', epochs=(0, 1, 2, 3, 4), losses=(5, 'bad', np.nan, 2, 1))
        experiment = self.monitor.discover_experiments()[0]
        self.assertEqual(self.monitor.load_experiment_data(experiment), (None, None))

    def test_epoch_selection_is_bounded_across_sparse_range(self):
        epochs = [0, 1, 5, 10, 25, 50, 100, 200, 500, 1000]
        chosen = select_representative_epochs(pd.DataFrame({'epoch': epochs}))
        self.assertEqual(len(chosen), 7)
        self.assertEqual((chosen[0], chosen[-1]), (0, 1000))
        self.assertTrue(set(chosen) <= set(epochs))

    def test_optional_metrics_are_included_only_when_recorded(self):
        epoch = pd.DataFrame({'epoch': [0, 1], 'avg_loss': [2, 1],
                              'policy_loss': [1, 0.5], 'elo': [np.nan, 1200],
                              'inference_ms': [10, 9]})
        stats = pd.DataFrame({'std': [0.1], 'var': [0.01]})
        slope = self.monitor.calculate_loss_slope_analysis(epoch)
        metrics = self.monitor.collect_metrics(epoch, stats, slope)
        self.assertEqual(metrics['training']['policy_loss'], 0.5)
        self.assertEqual(metrics['evaluation'], {'elo': 1200})
        self.assertEqual(metrics['runtime'], {'inference_ms': 9})
        self.assertNotIn('win_rate', metrics['evaluation'])


if __name__ == '__main__':
    unittest.main()
