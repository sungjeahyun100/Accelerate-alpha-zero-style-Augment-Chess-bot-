#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
통합 모델 모니터링 시스템
- 모든 실험에 대한 데이터 분석 및 시각화를 하나의 스크립트로 처리
- CSV 파일 생성 및 모든 그래프 자동 생성
- /model_result_monitoring/실험_모델_id 경로에 결과 저장
"""

import os
import pandas as pd
import numpy as np
import matplotlib.pyplot as plt
from pathlib import Path
import argparse
from datetime import datetime

EPS = 1e-12
MAX_INVALID_FRACTION = 0.2
OPTIONAL_METRIC_COLUMNS = {
    'training': ('policy_loss', 'value_loss'),
    'evaluation': ('win_rate', 'elo', 'policy_kl', 'value_mae', 'value_mse'),
    'runtime': ('inference_ms', 'positions_per_second',
                'mcts_simulations_per_second', 'memory_mb', 'peak_memory_mb',
                'parameter_count'),
}


def select_representative_epochs(batch_df, limit=7):
    """실제로 기록된 epoch에서 전체 구간의 대표값을 선택한다."""
    available = sorted(batch_df['epoch'].unique())
    if len(available) <= limit:
        return available
    indices = np.linspace(0, len(available) - 1, limit).round().astype(int)
    return [available[i] for i in sorted(set(indices))]


def format_metric(value, precision=6):
    """Markdown에 유효하지 않은 숫자를 그대로 표시하지 않는다."""
    return f'{value:.{precision}f}' if pd.notna(value) and np.isfinite(value) else 'N/A'

class ModelMonitor:
    """통합 모델 모니터링 클래스"""
    
    def __init__(self, workspace_dir, output_base_dir="model_result_monitoring"):
        self.workspace_dir = Path(workspace_dir)
        self.graph_dir = self.workspace_dir / "graph"
        self.output_base_dir = self.workspace_dir / output_base_dir
        self.output_base_dir.mkdir(exist_ok=True)
        
        # 그래프 스타일 설정
        plt.style.use('default')

    def source_timestamp(self, experiments, fmt):
        """입력 파일 수정 시각을 사용해 같은 입력의 리포트 내용을 일정하게 유지한다."""
        latest = max(path.stat().st_mtime for exp in experiments
                     for path in (exp['epoch_file'], exp['batch_file']))
        return datetime.fromtimestamp(latest).strftime(fmt)
        
    def discover_experiments(self):
        """모든 실험 데이터를 발견하고 정리"""
        experiments_by_id = {}
        
        if not self.graph_dir.exists():
            print(f"❌ 그래프 디렉토리가 없습니다: {self.graph_dir}")
            return []
        
        # 새 형식: graph/<실험ID>/ 형태 탐색
        for exp_dir in sorted(self.graph_dir.iterdir()):
            if exp_dir.is_dir():
                epoch_file = exp_dir / "epoch-loss.txt"
                batch_file = exp_dir / "batch-loss.txt"
                
                if epoch_file.exists() and batch_file.exists():
                    experiments_by_id[exp_dir.name] = {
                        'id': exp_dir.name,
                        'path': exp_dir,
                        'epoch_file': epoch_file,
                        'batch_file': batch_file,
                        'format': 'directory'
                    }
        
        # 구 형식도 항상 탐색하며, 같은 ID에는 directory 형식을 우선한다.
        for epoch_file in sorted(self.graph_dir.glob("epoch-loss-*.txt")):
            exp_id = epoch_file.stem.removeprefix("epoch-loss-")
            batch_file = self.graph_dir / f"batch-loss-{exp_id}.txt"
            if not batch_file.exists():
                continue
            if exp_id in experiments_by_id:
                print(f"⚠️ {exp_id}: 두 형식이 모두 있어 directory 형식을 사용합니다.")
                continue
            experiments_by_id[exp_id] = {
                'id': exp_id,
                'path': self.graph_dir,
                'epoch_file': epoch_file,
                'batch_file': batch_file,
                'format': 'flat'
            }
        
        # 수정 시간 기준으로 정렬 (최신 순)
        experiments = sorted(experiments_by_id.values(),
                             key=lambda x: (-x['epoch_file'].stat().st_mtime, x['id']))
        
        print(f"🔍 발견된 실험: {len(experiments)}개")
        for exp in experiments:
            print(f"   - {exp['id']} ({exp['format']} format)")
        
        return experiments
    
    def load_experiment_data(self, experiment):
        """실험 데이터 로드"""
        try:
            # New format with header and comma separator
            if experiment['format'] == 'directory':
                epoch_df = pd.read_csv(experiment['epoch_file']) # sep=',' is default
                batch_df = pd.read_csv(experiment['batch_file'])
            # Old format with space separator and no header
            else: # format == 'flat'
                epoch_df = pd.read_csv(experiment['epoch_file'], sep=r'\s+', header=None,
                                     names=['epoch', 'avg_loss'])
                batch_df = pd.read_csv(experiment['batch_file'], sep=r'\s+', header=None,
                                     names=['epoch', 'batch_num', 'loss'])

            return self.validate_experiment_data(experiment['id'], epoch_df, batch_df)
        
        except Exception as e:
            print(f"❌ {experiment['id']} 데이터 로드 실패: {e}")
            return None, None

    def validate_experiment_data(self, experiment_id, epoch_df, batch_df):
        """필수 schema, 숫자 및 slope 분석에 필요한 데이터 양을 확인한다."""
        for label, frame, required in (
            ('epoch', epoch_df, ('epoch', 'avg_loss')),
            ('batch', batch_df, ('epoch', 'batch_num', 'loss')),
        ):
            if frame.empty:
                raise ValueError(f'{label} 데이터가 비어 있습니다')
            missing = set(required) - set(frame.columns)
            if missing:
                raise ValueError(f'{label} 필수 컬럼 누락: {sorted(missing)}')
            for column in required:
                frame[column] = pd.to_numeric(frame[column], errors='coerce')
                frame[column] = frame[column].replace([np.inf, -np.inf], np.nan)
            invalid = frame[list(required)].isna().any(axis=1)
            fraction = invalid.mean()
            if fraction > MAX_INVALID_FRACTION:
                raise ValueError(f'{label} 잘못된 숫자/NaN 행이 {fraction:.1%}입니다')
            if invalid.any():
                print(f'⚠️ {experiment_id}: {label} 잘못된 숫자/NaN 행 {invalid.sum()}개 제외')
                frame.drop(index=frame.index[invalid], inplace=True)
            if frame.empty:
                raise ValueError(f'{label} 유효한 행이 없습니다')
        if epoch_df['epoch'].nunique() < 2:
            raise ValueError('epoch가 최소 2개 필요합니다')
        if epoch_df['epoch'].duplicated().any():
            raise ValueError('epoch 데이터에 중복 epoch가 있습니다')
        epoch_df = epoch_df.sort_values('epoch').reset_index(drop=True)
        batch_df = batch_df.sort_values(['epoch', 'batch_num']).reset_index(drop=True)
        return epoch_df, batch_df
    
    def calculate_epoch_statistics(self, batch_df):
        """에폭별 배치 손실 통계 계산"""
        epoch_stats = batch_df.groupby('epoch')['loss'].agg([
            'count',      # 배치 수
            'mean',       # 평균
            'std',        # 표준편차
            'var',        # 분산
            'min',        # 최솟값
            'max',        # 최댓값
            'median'      # 중앙값
        ]).reset_index()
        
        # 추가 통계
        epoch_stats['cv'] = np.nan
        valid_mean = epoch_stats['mean'].abs() > EPS
        epoch_stats.loc[valid_mean, 'cv'] = (
            epoch_stats.loc[valid_mean, 'std'] / epoch_stats.loc[valid_mean, 'mean'])
        epoch_stats['range'] = epoch_stats['max'] - epoch_stats['min']  # 범위
        
        return epoch_stats
    
    def calculate_loss_slope_analysis(self, epoch_df):
        """epoch에 따른 loss 변화율과 plateau 가능성 분석."""
        epochs = epoch_df['epoch'].values
        losses = epoch_df['avg_loss'].values
        
        # 1차, 2차 미분 계산
        dloss_depoch = np.gradient(losses, epochs)
        d2loss_depoch2 = np.gradient(dloss_depoch, epochs) if len(epochs) >= 3 else np.full(len(epochs), np.nan)
        
        # epoch별 loss 변화율의 크기
        abs_slope = np.abs(dloss_depoch)
        window = min(50, max(1, len(abs_slope) // 2))
        final_slope_magnitude = np.mean(abs_slope[-window:])
        initial_slope_magnitude = np.mean(abs_slope[:window])
        
        loss_slope_ratio = (final_slope_magnitude / initial_slope_magnitude
                            if initial_slope_magnitude > EPS else np.nan)
        
        # 수렴 상태 판정
        if final_slope_magnitude <= EPS:
            convergence_status = '거의 수렴'
        elif np.isfinite(loss_slope_ratio) and loss_slope_ratio < 0.1:
            convergence_status = 'plateau 진입 가능성'
        elif np.isfinite(loss_slope_ratio) and loss_slope_ratio < 0.5:
            convergence_status = '완만한 개선'
        elif losses[-1] < losses[0]:
            convergence_status = '활발한 개선'
        else:
            convergence_status = '개선 확인되지 않음'
        
        return {
            'dloss_depoch': dloss_depoch,
            'd2loss_depoch2': d2loss_depoch2,
            'abs_loss_slope': abs_slope,
            'loss_slope_ratio': loss_slope_ratio,
            'final_slope_magnitude': final_slope_magnitude,
            'initial_slope_magnitude': initial_slope_magnitude,
            'convergence_status': convergence_status,
            'recent_variance': np.var(losses[-100:]) if len(losses) >= 100 else np.var(losses),
            'total_improvement': losses[0] - losses[-1] if len(losses) > 0 else 0
        }
    
    def generate_comprehensive_csv(self, experiment_id, epoch_df, batch_df, epoch_stats, loss_slope_analysis, output_dir):
        """종합 CSV 파일 생성"""
        csv_files = {}
        
        # CSV 전용 디렉토리 생성
        csv_dir = output_dir / "csv_data"
        csv_dir.mkdir(exist_ok=True)
        
        # 1. 기본 에폭 데이터
        epoch_enhanced = epoch_df.copy()
        epoch_enhanced['loss_slope'] = loss_slope_analysis['dloss_depoch']
        epoch_enhanced['loss_slope_2nd'] = loss_slope_analysis['d2loss_depoch2']
        epoch_enhanced['abs_loss_slope'] = loss_slope_analysis['abs_loss_slope']
        
        csv_files['epoch_data'] = csv_dir / f"epoch_comprehensive_{experiment_id}.csv"
        epoch_enhanced.to_csv(csv_files['epoch_data'], index=False, encoding='utf-8')
        
        # 2. 에폭 통계
        csv_files['epoch_statistics'] = csv_dir / f"epoch_statistics_{experiment_id}.csv"
        epoch_stats.to_csv(csv_files['epoch_statistics'], index=False, encoding='utf-8')
        
        # 3. 배치 데이터 (샘플링)
        if len(batch_df) > 10000:  # 너무 크면 샘플링
            batch_sample = batch_df.sample(n=10000, random_state=42).sort_values(['epoch', 'batch_num'])
        else:
            batch_sample = batch_df
        
        csv_files['batch_data'] = csv_dir / f"batch_data_{experiment_id}.csv"
        batch_sample.to_csv(csv_files['batch_data'], index=False, encoding='utf-8')
        
        # 4. 요약 통계
        summary_data = {
            'experiment_id': [experiment_id],
            'total_epochs': [len(epoch_df)],
            'total_batches': [len(batch_df)],
            'initial_loss': [epoch_df['avg_loss'].iloc[0]],
            'final_loss': [epoch_df['avg_loss'].iloc[-1]],
            'min_loss': [epoch_df['avg_loss'].min()],
            'loss_slope_ratio': [loss_slope_analysis['loss_slope_ratio']],
            'convergence_status': [loss_slope_analysis['convergence_status']],
            'total_improvement': [loss_slope_analysis['total_improvement']],
            'avg_batch_std': [epoch_stats['std'].mean()],
            'avg_batch_var': [epoch_stats['var'].mean()],
            'generation_time': [self.generation_time]
        }
        
        summary_df = pd.DataFrame(summary_data)
        csv_files['summary'] = csv_dir / f"experiment_summary_{experiment_id}.csv"
        summary_df.to_csv(csv_files['summary'], index=False, encoding='utf-8')
        
        return csv_files
    
    def create_loss_plots(self, experiment_id, epoch_df, batch_df, output_dir):
        """손실 관련 그래프 생성"""
        # 그래프 전용 디렉토리 생성
        plots_dir = output_dir / "plots"
        plots_dir.mkdir(exist_ok=True)
        
        # 1. 에폭 평균 손실
        fig, ax = plt.subplots(figsize=(12, 8))
        ax.plot(epoch_df['epoch'], epoch_df['avg_loss'], 'b-', linewidth=2, marker='o', markersize=3)
        ax.set_title(f'Epoch Average Loss - {experiment_id}', fontsize=16, fontweight='bold')
        ax.set_xlabel('Epoch', fontsize=14)
        ax.set_ylabel('Average Loss', fontsize=14)
        ax.grid(True, alpha=0.3)
        
        plt.tight_layout()
        loss_plot_path = plots_dir / f"epoch_loss_{experiment_id}.png"
        plt.savefig(loss_plot_path, dpi=300, bbox_inches='tight')
        plt.close()
        
        # 실제 배치 데이터에 존재하는 epoch만 균등하게 선택한다.
        selected_epochs = select_representative_epochs(batch_df)
        
        if selected_epochs:
            fig, ax = plt.subplots(figsize=(15, 10))
            
            colors = plt.cm.tab10(np.linspace(0, 1, len(selected_epochs)))
            
            for i, epoch in enumerate(selected_epochs):
                epoch_batches = batch_df[batch_df['epoch'] == epoch]
                ax.plot(epoch_batches['batch_num'], epoch_batches['loss'], 
                       color=colors[i], linewidth=1.5, marker='o', markersize=2,
                       label=f'Epoch {epoch}', alpha=0.8)
            
            ax.set_title(f'Batch Loss Comparison - {experiment_id}', fontsize=16, fontweight='bold')
            ax.set_xlabel('Batch Number', fontsize=14)
            ax.set_ylabel('Loss', fontsize=14)
            ax.legend(bbox_to_anchor=(1.05, 1), loc='upper left')
            ax.grid(True, alpha=0.3)
            
            plt.tight_layout()
            batch_plot_path = plots_dir / f"batch_loss_{experiment_id}.png"
            plt.savefig(batch_plot_path, dpi=300, bbox_inches='tight')
            plt.close()
        
        return loss_plot_path, batch_plot_path if selected_epochs else None
    
    def create_loss_slope_plots(self, experiment_id, epoch_df, loss_slope_analysis, output_dir):
        """Loss slope / plateau 분석 그래프 생성"""
        # 그래프 전용 디렉토리 사용
        plots_dir = output_dir / "plots"
        plots_dir.mkdir(exist_ok=True)
        
        epochs = epoch_df['epoch'].values
        losses = epoch_df['avg_loss'].values
        dloss_depoch = loss_slope_analysis['dloss_depoch']
        d2loss_depoch2 = loss_slope_analysis['d2loss_depoch2']
        abs_slope = loss_slope_analysis['abs_loss_slope']
        
        fig, axes = plt.subplots(2, 2, figsize=(16, 12))
        
        # 1. 원본 Loss 곡선
        axes[0, 0].plot(epochs, losses, 'b-', linewidth=2)
        axes[0, 0].set_title('Loss vs Epoch')
        axes[0, 0].set_xlabel('Epoch')
        axes[0, 0].set_ylabel('Loss')
        axes[0, 0].grid(True, alpha=0.3)
        
        # 2. epoch에 따른 loss 변화율
        axes[0, 1].plot(epochs, dloss_depoch, 'r-', linewidth=2)
        axes[0, 1].set_title('Loss Slope (dLoss/dEpoch)')
        axes[0, 1].set_xlabel('Epoch')
        axes[0, 1].set_ylabel('Loss Slope')
        axes[0, 1].grid(True, alpha=0.3)
        axes[0, 1].axhline(y=0, color='k', linestyle='--', alpha=0.5)
        
        # 3. loss 변화율 크기
        axes[1, 0].plot(epochs, abs_slope, 'g-', linewidth=2)
        axes[1, 0].set_title('Absolute Loss Slope')
        axes[1, 0].set_xlabel('Epoch')
        axes[1, 0].set_ylabel('|dLoss/dEpoch|')
        axes[1, 0].set_yscale('symlog', linthresh=EPS)
        axes[1, 0].grid(True, alpha=0.3)
        
        # 4. 2차 미분
        axes[1, 1].plot(epochs, d2loss_depoch2, 'm-', linewidth=2)
        axes[1, 1].set_title('Second Derivative (d²Loss/dEpoch²)')
        axes[1, 1].set_xlabel('Epoch')
        axes[1, 1].set_ylabel('Second Derivative')
        axes[1, 1].grid(True, alpha=0.3)
        axes[1, 1].axhline(y=0, color='k', linestyle='--', alpha=0.5)
        
        plt.suptitle(f'Loss Slope / Plateau Analysis: {experiment_id}', fontsize=16, fontweight='bold')
        plt.tight_layout()
        
        slope_plot_path = plots_dir / f"loss_slope_analysis_{experiment_id}.png"
        plt.savefig(slope_plot_path, dpi=300, bbox_inches='tight')
        plt.close()
        
        return slope_plot_path
    
    def create_statistics_plots(self, experiment_id, epoch_stats, batch_df, output_dir):
        """통계 분석 그래프 생성"""
        # 그래프 전용 디렉토리 사용
        plots_dir = output_dir / "plots"
        plots_dir.mkdir(exist_ok=True)
        
        # 1. 에폭별 통계 트렌드
        fig, axes = plt.subplots(2, 2, figsize=(16, 12))
        
        # 표준편차 트렌드
        axes[0, 0].plot(epoch_stats['epoch'], epoch_stats['std'], 'b-', linewidth=2)
        axes[0, 0].set_title('Standard Deviation per Epoch')
        axes[0, 0].set_xlabel('Epoch')
        axes[0, 0].set_ylabel('Standard Deviation')
        axes[0, 0].grid(True, alpha=0.3)
        
        # 분산 트렌드
        axes[0, 1].plot(epoch_stats['epoch'], epoch_stats['var'], 'r-', linewidth=2)
        axes[0, 1].set_title('Variance per Epoch')
        axes[0, 1].set_xlabel('Epoch')
        axes[0, 1].set_ylabel('Variance')
        axes[0, 1].grid(True, alpha=0.3)
        
        # 변동계수 트렌드
        axes[1, 0].plot(epoch_stats['epoch'], epoch_stats['cv'], 'g-', linewidth=2)
        axes[1, 0].set_title('Coefficient of Variation per Epoch')
        axes[1, 0].set_xlabel('Epoch')
        axes[1, 0].set_ylabel('CV (std/mean)')
        axes[1, 0].grid(True, alpha=0.3)
        
        # 범위 트렌드
        axes[1, 1].plot(epoch_stats['epoch'], epoch_stats['range'], 'm-', linewidth=2)
        axes[1, 1].set_title('Loss Range per Epoch')
        axes[1, 1].set_xlabel('Epoch')
        axes[1, 1].set_ylabel('Range (max - min)')
        axes[1, 1].grid(True, alpha=0.3)
        
        plt.suptitle(f'Statistics Trends: {experiment_id}', fontsize=16, fontweight='bold')
        plt.tight_layout()
        
        stats_plot_path = plots_dir / f"statistics_trends_{experiment_id}.png"
        plt.savefig(stats_plot_path, dpi=300, bbox_inches='tight')
        plt.close()
        
        # 2. 통계 분포 히스토그램
        fig, axes = plt.subplots(2, 2, figsize=(16, 12))
        
        # 표준편차 분포
        axes[0, 0].hist(epoch_stats['std'].dropna(), bins=30, alpha=0.7, color='blue', edgecolor='black')
        axes[0, 0].set_title('Distribution of Standard Deviations')
        axes[0, 0].set_xlabel('Standard Deviation')
        axes[0, 0].set_ylabel('Frequency')
        axes[0, 0].grid(True, alpha=0.3)
        
        # 분산 분포
        axes[0, 1].hist(epoch_stats['var'].dropna(), bins=30, alpha=0.7, color='red', edgecolor='black')
        axes[0, 1].set_title('Distribution of Variances')
        axes[0, 1].set_xlabel('Variance')
        axes[0, 1].set_ylabel('Frequency')
        axes[0, 1].grid(True, alpha=0.3)
        
        # 변동계수 분포
        axes[1, 0].hist(epoch_stats['cv'].dropna(), bins=30, alpha=0.7, color='green', edgecolor='black')
        axes[1, 0].set_title('Distribution of Coefficient of Variation')
        axes[1, 0].set_xlabel('CV (std/mean)')
        axes[1, 0].set_ylabel('Frequency')
        axes[1, 0].grid(True, alpha=0.3)
        
        # 상관관계 히트맵
        corr_cols = ['mean', 'std', 'var', 'cv', 'range']
        correlation_matrix = epoch_stats[corr_cols].corr()
        im = axes[1, 1].imshow(correlation_matrix, cmap='RdBu_r', aspect='auto', vmin=-1, vmax=1)
        axes[1, 1].set_xticks(range(len(corr_cols)))
        axes[1, 1].set_yticks(range(len(corr_cols)))
        axes[1, 1].set_xticklabels(corr_cols, rotation=45)
        axes[1, 1].set_yticklabels(corr_cols)
        axes[1, 1].set_title('Correlation Matrix')
        
        # 상관관계 값 표시
        for i in range(len(corr_cols)):
            for j in range(len(corr_cols)):
                axes[1, 1].text(j, i, format_metric(correlation_matrix.iloc[i, j], 2),
                                     ha="center", va="center", color="black", fontsize=8)
        
        plt.colorbar(im, ax=axes[1, 1], shrink=0.8)
        plt.suptitle(f'Statistics Distributions: {experiment_id}', fontsize=16, fontweight='bold')
        plt.tight_layout()
        
        dist_plot_path = plots_dir / f"statistics_distributions_{experiment_id}.png"
        plt.savefig(dist_plot_path, dpi=300, bbox_inches='tight')
        plt.close()
        
        return stats_plot_path, dist_plot_path
    
    def generate_experiment_report(self, experiment_id, epoch_df, batch_df, epoch_stats, loss_slope_analysis, csv_files, plot_files, output_dir):
        """실험 리포트 생성"""
        report_path = output_dir / f"experiment_report_{experiment_id}.md"
        
        with open(report_path, 'w', encoding='utf-8') as f:
            f.write(f"# 실험 리포트: {experiment_id}\n\n")
            f.write(f"**입력 파일 수정 시각**: {self.generation_time}\n\n")
            
            # 실험 개요
            f.write("## 📊 실험 개요\n\n")
            f.write(f"- **총 에폭 수**: {len(epoch_df)}\n")
            f.write(f"- **총 배치 수**: {len(batch_df)}\n")
            f.write(f"- **초기 손실**: {epoch_df['avg_loss'].iloc[0]:.6f}\n")
            f.write(f"- **최종 손실**: {epoch_df['avg_loss'].iloc[-1]:.6f}\n")
            f.write(f"- **최소 손실**: {epoch_df['avg_loss'].min():.6f}\n")
            f.write(f"- **총 개선량**: {loss_slope_analysis['total_improvement']:.6f}\n\n")
            
            # dLoss/dEpoch는 parameter gradient가 아니다.
            f.write("## 🔍 Loss Slope / Plateau Analysis\n\n")
            f.write("epoch별 loss 변화율이며 layer별 parameter gradient를 측정하지 않습니다.\n\n")
            f.write(f"- **Loss slope 비율 (최종/초기)**: {format_metric(loss_slope_analysis['loss_slope_ratio'], 4)}\n")
            f.write(f"- **수렴 상태**: {loss_slope_analysis['convergence_status']}\n")
            f.write(f"- **초기 loss slope 크기**: {format_metric(loss_slope_analysis['initial_slope_magnitude'], 8)}\n")
            f.write(f"- **최종 loss slope 크기**: {format_metric(loss_slope_analysis['final_slope_magnitude'], 8)}\n")
            f.write(f"- **최근 분산**: {format_metric(loss_slope_analysis['recent_variance'], 8)}\n\n")
            
            # 통계 요약
            f.write("## 📈 배치 손실 통계\n\n")
            f.write(f"- **평균 표준편차**: {format_metric(epoch_stats['std'].mean())}\n")
            f.write(f"- **평균 분산**: {format_metric(epoch_stats['var'].mean())}\n")
            f.write(f"- **평균 변동계수**: {format_metric(epoch_stats['cv'].mean())}\n")
            f.write(f"- **표준편차 범위**: {format_metric(epoch_stats['std'].min())} ~ {format_metric(epoch_stats['std'].max())}\n\n")
            
            # 생성된 파일들
            f.write("## 📁 생성된 파일들\n\n")
            f.write("### CSV 데이터 (csv_data/ 폴더)\n")
            for file_type, file_path in csv_files.items():
                f.write(f"- **{file_type}**: `csv_data/{file_path.name}`\n")
            
            f.write("\n### 그래프 (plots/ 폴더)\n")
            for plot_desc, plot_path in plot_files.items():
                if plot_path:
                    f.write(f"- **{plot_desc}**: `plots/{plot_path.name}`\n")
            
            f.write(f"\n---\n")
            f.write(f"*리포트 생성: Unified Model Monitor v1.0*\n")
        
        return report_path
    
    def process_experiment(self, experiment):
        """단일 실험 처리"""
        experiment_id = experiment['id']
        print(f"\n{'='*60}")
        print(f"📊 실험 처리 중: {experiment_id}")
        print(f"{'='*60}")
        
        # 데이터 로드
        print("📂 데이터 로드 중...")
        epoch_df, batch_df = self.load_experiment_data(experiment)
        if epoch_df is None or batch_df is None:
            return None

        self.generation_time = self.source_timestamp([experiment], '%Y-%m-%d %H:%M:%S')

        output_dir = self.output_base_dir / experiment_id
        output_dir.mkdir(exist_ok=True)
        
        print(f"   - 에폭 데이터: {len(epoch_df)} 개")
        print(f"   - 배치 데이터: {len(batch_df)} 개")
        
        # 통계 계산
        print("🔢 통계 계산 중...")
        epoch_stats = self.calculate_epoch_statistics(batch_df)
        loss_slope_analysis = self.calculate_loss_slope_analysis(epoch_df)
        
        # CSV 파일 생성
        print("💾 CSV 파일 생성 중...")
        csv_files = self.generate_comprehensive_csv(
            experiment_id, epoch_df, batch_df, epoch_stats, loss_slope_analysis, output_dir
        )
        
        # 그래프 생성
        print("📈 그래프 생성 중...")
        
        # 손실 그래프
        loss_plot, batch_plot = self.create_loss_plots(experiment_id, epoch_df, batch_df, output_dir)
        
        slope_plot = self.create_loss_slope_plots(experiment_id, epoch_df, loss_slope_analysis, output_dir)
        
        # 통계 그래프
        stats_plot, dist_plot = self.create_statistics_plots(experiment_id, epoch_stats, batch_df, output_dir)
        
        plot_files = {
            "에폭 평균 손실": loss_plot,
            "배치 손실 비교": batch_plot,
            "Loss slope / plateau 분석": slope_plot,
            "통계 트렌드": stats_plot,
            "통계 분포": dist_plot
        }
        
        # 실험 리포트 생성
        print("📝 실험 리포트 생성 중...")
        report_path = self.generate_experiment_report(
            experiment_id, epoch_df, batch_df, epoch_stats, loss_slope_analysis, csv_files, plot_files, output_dir
        )
        
        print(f"✅ 완료! 결과 저장: {output_dir}")
        print(f"📋 리포트: {report_path.name}")
        
        return {
            'experiment_id': experiment_id,
            'output_dir': output_dir,
            'epoch_df': epoch_df,
            'batch_df': batch_df,
            'epoch_stats': epoch_stats,
            'loss_slope_analysis': loss_slope_analysis,
            'metrics': self.collect_metrics(epoch_df, epoch_stats, loss_slope_analysis),
            'source_experiment': experiment,
            'csv_files': csv_files,
            'plot_files': plot_files,
            'report_path': report_path
        }

    def collect_metrics(self, epoch_df, epoch_stats, slope):
        """Training, evaluation, runtime 지표 중 실제 기록된 값만 수집한다."""
        metrics = {
            'training': {
                'initial_loss': epoch_df['avg_loss'].iloc[0],
                'final_loss': epoch_df['avg_loss'].iloc[-1],
                'min_loss': epoch_df['avg_loss'].min(),
                'total_improvement': slope['total_improvement'],
                'loss_slope_ratio': slope['loss_slope_ratio'],
                'convergence_status': slope['convergence_status'],
                'avg_batch_std': epoch_stats['std'].mean(),
                'avg_batch_var': epoch_stats['var'].mean(),
            },
            'evaluation': {},
            'runtime': {},
        }
        # Optional columns may arrive in future epoch files. Only recorded finite
        # values are surfaced; separate evaluation/runtime files can be joined here.
        for category, columns in OPTIONAL_METRIC_COLUMNS.items():
            for column in columns:
                if column not in epoch_df:
                    continue
                values = pd.to_numeric(epoch_df[column], errors='coerce')
                values = values.replace([np.inf, -np.inf], np.nan).dropna()
                if not values.empty:
                    metrics[category][column] = values.iloc[-1]
        return metrics

    def build_comparison_data(self, processed_experiments):
        """기존 CSV 열을 유지하며 새 범주의 실제 지표만 추가한다."""
        rows = []
        for exp in processed_experiments:
            metrics = exp['metrics']
            row = {'experiment_id': exp['experiment_id'],
                   'total_epochs': len(exp['epoch_df']), **metrics['training']}
            for category in ('evaluation', 'runtime'):
                row.update({f'{category}_{key}': value
                            for key, value in metrics[category].items()})
            rows.append(row)
        return pd.DataFrame(rows)
    
    def generate_comparison_report(self, processed_experiments):
        """전체 실험 비교 리포트 생성"""
        if not processed_experiments:
            return
        
        print(f"\n{'='*80}")
        print("🏆 전체 실험 비교 리포트 생성")
        print(f"{'='*80}")
        
        comparison_df = self.build_comparison_data(processed_experiments)
        
        # 통합 CSV 디렉토리 생성
        consolidated_csv_dir = self.output_base_dir / "consolidated_csv"
        consolidated_csv_dir.mkdir(exist_ok=True)
        
        # 비교 CSV 저장
        comparison_csv = consolidated_csv_dir / "experiments_comparison.csv"
        comparison_df.to_csv(comparison_csv, index=False, encoding='utf-8')
        
        # 모든 실험의 CSV 데이터를 통합
        self.consolidate_all_csv_data(processed_experiments, consolidated_csv_dir)
        
        # 비교 리포트 생성
        report_path = self.output_base_dir / "comparison_report.md"
        with open(report_path, 'w', encoding='utf-8') as f:
            f.write(f"# 전체 실험 비교 리포트\n\n")
            source_time = self.source_timestamp(
                [exp['source_experiment'] for exp in processed_experiments],
                '%Y-%m-%d %H:%M:%S')
            f.write(f"**입력 파일 수정 시각**: {source_time}\n")
            f.write(f"**분석 실험 수**: {len(processed_experiments)}\n\n")
            
            # training loss 개선량 내림차순. 동률은 실험 ID로 고정한다.
            ranked = comparison_df.sort_values(
                ['total_improvement', 'experiment_id'], ascending=[False, True])
            
            f.write("## 🎯 Loss 개선량 기준 순위 (training loss)\n\n")
            f.write("| 순위 | 실험 ID | Loss slope 비율 | 수렴 상태 | 총 개선량 |\n")
            f.write("|------|---------|-------------|-----------|----------|\n")
            
            for i, (_, row) in enumerate(ranked.iterrows(), 1):
                f.write(f"| {i} | {row['experiment_id']} | {format_metric(row['loss_slope_ratio'], 4)} | {row['convergence_status']} | {format_metric(row['total_improvement'])} |\n")
            
            best_exp = ranked.iloc[0]
            worst_exp = ranked.iloc[-1]
            
            f.write(f"\n## 🏆 Loss 개선량이 가장 큰 실험\n\n")
            f.write(f"**실험 ID**: {best_exp['experiment_id']}\n")
            f.write(f"- Loss slope 비율: {format_metric(best_exp['loss_slope_ratio'], 4)}\n")
            f.write(f"- 수렴 상태: {best_exp['convergence_status']}\n")
            f.write(f"- 총 개선량: {best_exp['total_improvement']:.6f}\n")
            
            f.write(f"\n## 📉 Loss 개선량이 가장 작은 실험\n\n")
            f.write(f"**실험 ID**: {worst_exp['experiment_id']}\n")
            f.write(f"- Loss slope 비율: {format_metric(worst_exp['loss_slope_ratio'], 4)}\n")
            f.write(f"- 수렴 상태: {worst_exp['convergence_status']}\n")
            f.write(f"- 총 개선량: {worst_exp['total_improvement']:.6f}\n")
            
            f.write(f"\n## 📊 전체 통계\n\n")
            f.write(f"- **평균 Loss slope 비율**: {format_metric(comparison_df['loss_slope_ratio'].mean(), 4)}\n")
            f.write(f"- **평균 총 개선량**: {format_metric(comparison_df['total_improvement'].mean())}\n")
            f.write(f"- **평균 최종 손실**: {format_metric(comparison_df['final_loss'].mean())}\n")
            
            f.write(f"\n## 📁 통합 CSV 파일\n\n")
            f.write(f"모든 실험의 CSV 데이터가 `consolidated_csv/` 폴더에 통합되었습니다:\n")
            f.write(f"- **실험 비교**: `experiments_comparison.csv`\n")
            f.write(f"- **모든 에폭 데이터**: `all_epochs_comprehensive.csv`\n")
            f.write(f"- **모든 에폭 통계**: `all_epochs_statistics.csv`\n")
            f.write(f"- **모든 실험 요약**: `all_experiments_summary.csv`\n")
        
        print(f"✅ 비교 리포트 생성: {report_path}")
        print(f"✅ 비교 CSV 생성: {comparison_csv}")
        print(f"✅ 통합 CSV 생성: {consolidated_csv_dir}")
    
    def consolidate_all_csv_data(self, processed_experiments, output_dir):
        """모든 실험의 CSV 데이터를 통합"""
        all_epochs_data = []
        all_stats_data = []
        all_summary_data = []
        
        for exp in processed_experiments:
            experiment_id = exp['experiment_id']
            
            # 에폭 데이터 통합
            epoch_enhanced = exp['epoch_df'].copy()
            epoch_enhanced['loss_slope'] = exp['loss_slope_analysis']['dloss_depoch']
            epoch_enhanced['loss_slope_2nd'] = exp['loss_slope_analysis']['d2loss_depoch2']
            epoch_enhanced['abs_loss_slope'] = exp['loss_slope_analysis']['abs_loss_slope']
            epoch_enhanced['experiment_id'] = experiment_id
            all_epochs_data.append(epoch_enhanced)
            
            # 통계 데이터 통합
            stats_data = exp['epoch_stats'].copy()
            stats_data['experiment_id'] = experiment_id
            all_stats_data.append(stats_data)
            
            # 요약 데이터는 이미 수집되어 있음
        
        # 통합 DataFrame 생성 및 저장
        if all_epochs_data:
            all_epochs_df = pd.concat(all_epochs_data, ignore_index=True)
            all_epochs_csv = output_dir / "all_epochs_comprehensive.csv"
            all_epochs_df.to_csv(all_epochs_csv, index=False, encoding='utf-8')
            print(f"✅ 통합 에폭 데이터: {all_epochs_csv}")
        
        if all_stats_data:
            all_stats_df = pd.concat(all_stats_data, ignore_index=True)
            all_stats_csv = output_dir / "all_epochs_statistics.csv"
            all_stats_df.to_csv(all_stats_csv, index=False, encoding='utf-8')
            print(f"✅ 통합 통계 데이터: {all_stats_csv}")
        
        # 요약 데이터 통합 (개별 요약 파일들을 읽어서 통합)
        all_summary_data = []
        for exp in processed_experiments:
            summary_file = exp['csv_files']['summary']
            if summary_file.exists():
                summary_df = pd.read_csv(summary_file)
                all_summary_data.append(summary_df)
        
        if all_summary_data:
            all_summary_df = pd.concat(all_summary_data, ignore_index=True)
            all_summary_csv = output_dir / "all_experiments_summary.csv"
            all_summary_df.to_csv(all_summary_csv, index=False, encoding='utf-8')
            print(f"✅ 통합 요약 데이터: {all_summary_csv}")
    
    def run(self, experiment_ids=None, latest_only=False):
        """메인 실행 함수"""
        print("🚀 통합 모델 모니터링 시스템 시작")
        print(f"📂 작업 디렉토리: {self.workspace_dir}")
        print(f"💾 출력 디렉토리: {self.output_base_dir}")
        
        # 실험 발견
        experiments = self.discover_experiments()
        if not experiments:
            print("❌ 분석할 실험이 없습니다.")
            return
        
        # 실험 필터링
        if latest_only:
            experiments = experiments[:1]
            print(f"🎯 최신 실험만 처리: {experiments[0]['id']}")
        elif experiment_ids:
            experiments = [exp for exp in experiments if exp['id'] in experiment_ids]
            print(f"🎯 지정된 실험만 처리: {[exp['id'] for exp in experiments]}")
        
        if not experiments:
            print("❌ 처리할 실험이 없습니다.")
            return
        
        # 각 실험 처리
        processed_experiments = []
        for experiment in experiments:
            try:
                result = self.process_experiment(experiment)
                if result:
                    processed_experiments.append(result)
            except Exception as e:
                print(f"❌ {experiment['id']} 처리 실패: {e}")
        
        # 전체 비교 리포트 생성
        if len(processed_experiments) > 1:
            self.generate_comparison_report(processed_experiments)
        
        print(f"\n🎉 모든 처리 완료!")
        print(f"📁 결과 위치: {self.output_base_dir}")
        print(f"✅ 처리된 실험: {len(processed_experiments)}개")

def main():
    parser = argparse.ArgumentParser(description='통합 모델 모니터링 시스템')
    parser.add_argument('--workspace', default=os.getcwd(),
                       help='작업 디렉토리 경로 (기본값: 현재 디렉토리)')
    parser.add_argument('--output-dir', default='model_result_monitoring',
                       help='출력 디렉토리 이름 (기본값: model_result_monitoring)')
    parser.add_argument('--experiments', nargs='+',
                       help='처리할 특정 실험 ID들 (기본값: 모든 실험)')
    parser.add_argument('--latest-only', action='store_true',
                       help='최신 실험만 처리')
    
    args = parser.parse_args()
    
    # 모니터 생성 및 실행
    monitor = ModelMonitor(workspace_dir=args.workspace, output_base_dir=args.output_dir)
    monitor.run(experiment_ids=args.experiments, latest_only=args.latest_only)

if __name__ == "__main__":
    main()
