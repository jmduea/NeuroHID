"""
Decoder Module - Reinforcement Learning Policy for Brain-Computer Interface

This module contains the neural network decoder that translates EEG features
into HID actions (mouse movements, clicks, keystrokes). We use Proximal Policy
Optimization (PPO), a stable and well-understood RL algorithm.

How the Decoder Works:
---------------------
The decoder is a neural network that takes in a feature vector (extracted from
your EEG signals) and outputs a probability distribution over possible actions.
For continuous actions like mouse movement, it outputs a mean and variance for
a Gaussian distribution. For discrete actions like clicks, it outputs 
probabilities for each possible action.

The learning loop works like this:
1. Receive features from Rust (via IPC)
2. Run the policy network to get an action distribution
3. Sample an action from the distribution
4. Send the action back to Rust for execution
5. Later, receive a reward signal (from ErrP detection)
6. Use the reward to update the policy (make good actions more likely)

Why PPO?
--------
PPO is our algorithm of choice for several reasons:
- It's stable and doesn't require careful hyperparameter tuning
- It works well with continuous action spaces (mouse movement)
- It can handle noisy, sparse rewards (which is what we get from ErrP)
- There's extensive research and battle-tested implementations available
"""

from dataclasses import dataclass
from typing import Optional, Tuple, List
import torch
import torch.nn as nn
import torch.nn.functional as F
import numpy as np


@dataclass
class DecoderConfig:
    """Configuration for the decoder network and training.
    
    These hyperparameters control how the neural network is structured and
    how it learns. The defaults are tuned for our use case (small networks,
    noisy rewards, online learning), but you may want to adjust them based
    on your specific hardware and signal quality.
    """
    
    # Network architecture
    input_dim: int = 20          # Number of features from signal processing
    hidden_dims: List[int] = None  # Hidden layer sizes (default: [64, 64])
    
    # Action space configuration
    continuous_action_dim: int = 2   # Mouse movement (dx, dy)
    discrete_action_count: int = 5   # No-op, left click, right click, arrow keys
    
    # PPO hyperparameters
    learning_rate: float = 3e-4
    gamma: float = 0.99              # Discount factor for future rewards
    gae_lambda: float = 0.95         # GAE lambda for advantage estimation
    clip_epsilon: float = 0.2        # PPO clipping parameter
    entropy_coef: float = 0.01       # Encourages exploration
    value_coef: float = 0.5          # Weight for value function loss
    max_grad_norm: float = 0.5       # Gradient clipping
    
    # Training configuration
    batch_size: int = 32
    update_epochs: int = 4           # PPO epochs per update
    
    def __post_init__(self):
        if self.hidden_dims is None:
            self.hidden_dims = [64, 64]


class PolicyNetwork(nn.Module):
    """The neural network that maps observations to actions.
    
    This is an actor-critic network, meaning it has two "heads":
    1. The actor (policy) head: outputs action probabilities/parameters
    2. The critic (value) head: estimates how good the current state is
    
    Having both in one network lets them share representations, which is
    more parameter-efficient than having separate networks.
    """
    
    def __init__(self, config: DecoderConfig):
        super().__init__()
        self.config = config
        
        # Build the shared feature extractor (the "backbone")
        # This processes the raw features into a representation that both
        # the actor and critic can use.
        layers = []
        prev_dim = config.input_dim
        for hidden_dim in config.hidden_dims:
            layers.append(nn.Linear(prev_dim, hidden_dim))
            layers.append(nn.ReLU())
            prev_dim = hidden_dim
        
        self.backbone = nn.Sequential(*layers)
        
        # Actor head for continuous actions (mouse movement)
        # We output mean and log_std for a Gaussian distribution
        self.continuous_mean = nn.Linear(prev_dim, config.continuous_action_dim)
        self.continuous_log_std = nn.Parameter(
            torch.zeros(config.continuous_action_dim)
        )
        
        # Actor head for discrete actions (clicks, key presses)
        # We output logits for a categorical distribution
        self.discrete_logits = nn.Linear(prev_dim, config.discrete_action_count)
        
        # Critic head (value function)
        # Estimates the expected return from the current state
        self.value_head = nn.Linear(prev_dim, 1)
    
    def forward(
        self, 
        features: torch.Tensor
    ) -> Tuple[torch.Tensor, torch.Tensor, torch.Tensor, torch.Tensor]:
        """Forward pass through the network.
        
        Args:
            features: Batch of feature vectors [batch_size, input_dim]
            
        Returns:
            Tuple of:
            - continuous_mean: Mean of Gaussian for continuous actions
            - continuous_log_std: Log std of Gaussian for continuous actions
            - discrete_logits: Logits for discrete action distribution
            - value: Estimated state value
        """
        # Shared feature extraction
        hidden = self.backbone(features)
        
        # Actor outputs
        continuous_mean = self.continuous_mean(hidden)
        continuous_log_std = self.continuous_log_std.expand_as(continuous_mean)
        discrete_logits = self.discrete_logits(hidden)
        
        # Critic output
        value = self.value_head(hidden).squeeze(-1)
        
        return continuous_mean, continuous_log_std, discrete_logits, value
    
    def get_action(
        self, 
        features: torch.Tensor,
        deterministic: bool = False
    ) -> Tuple[torch.Tensor, torch.Tensor, torch.Tensor]:
        """Sample an action from the policy.
        
        Args:
            features: Single feature vector or batch [batch_size, input_dim]
            deterministic: If True, return the mean instead of sampling
            
        Returns:
            Tuple of:
            - continuous_action: The continuous action (mouse movement)
            - discrete_action: The discrete action (click, key, or no-op)
            - log_prob: Log probability of the action (for PPO)
        """
        continuous_mean, continuous_log_std, discrete_logits, _ = self.forward(features)
        
        # Sample continuous action from Gaussian
        if deterministic:
            continuous_action = continuous_mean
        else:
            continuous_std = continuous_log_std.exp()
            continuous_dist = torch.distributions.Normal(continuous_mean, continuous_std)
            continuous_action = continuous_dist.sample()
        
        # Sample discrete action from categorical
        discrete_dist = torch.distributions.Categorical(logits=discrete_logits)
        if deterministic:
            discrete_action = discrete_logits.argmax(dim=-1)
        else:
            discrete_action = discrete_dist.sample()
        
        # Compute log probabilities for PPO
        continuous_log_prob = torch.distributions.Normal(
            continuous_mean, continuous_log_std.exp()
        ).log_prob(continuous_action).sum(dim=-1)
        discrete_log_prob = discrete_dist.log_prob(discrete_action)
        
        # Combined log probability
        log_prob = continuous_log_prob + discrete_log_prob
        
        return continuous_action, discrete_action, log_prob


class Decoder:
    """High-level decoder interface for the NeuroHID system.
    
    This class wraps the policy network and provides methods for:
    - Inference: Getting actions from features
    - Training: Updating the policy from experiences
    - Saving/Loading: Persisting the trained model
    
    Example usage:
        config = DecoderConfig(input_dim=20)
        decoder = Decoder(config)
        
        # Get an action from features
        features = np.array([...])  # 20-dimensional feature vector
        action = decoder.get_action(features)
        
        # Train from a batch of experiences
        decoder.train_step(observations, actions, rewards, dones)
    """
    
    def __init__(self, config: DecoderConfig):
        self.config = config
        self.device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
        
        # Create the policy network
        self.policy = PolicyNetwork(config).to(self.device)
        
        # Optimizer
        self.optimizer = torch.optim.Adam(
            self.policy.parameters(),
            lr=config.learning_rate
        )
        
        # Experience buffer for PPO updates
        self.experience_buffer = ExperienceBuffer()
    
    def get_action(self, features: np.ndarray, deterministic: bool = False) -> dict:
        """Get an action from the decoder given features.
        
        Args:
            features: Feature vector from signal processing
            deterministic: If True, use the mean action (no exploration)
            
        Returns:
            Dictionary with:
            - 'mouse_dx': Horizontal mouse movement
            - 'mouse_dy': Vertical mouse movement  
            - 'discrete': Index of discrete action (0=no-op, 1=left click, etc.)
            - 'confidence': How confident the decoder is in this action
        """
        # Convert to tensor
        features_tensor = torch.FloatTensor(features).unsqueeze(0).to(self.device)
        
        # Get action from policy
        with torch.no_grad():
            continuous, discrete, log_prob = self.policy.get_action(
                features_tensor, deterministic
            )
        
        # Compute confidence from log probability
        # Higher log_prob = more confident
        confidence = torch.sigmoid(log_prob).item()
        
        return {
            'mouse_dx': continuous[0, 0].item(),
            'mouse_dy': continuous[0, 1].item(),
            'discrete': discrete[0].item(),
            'confidence': confidence,
        }
    
    def add_experience(
        self,
        features: np.ndarray,
        action: dict,
        reward: float,
        done: bool
    ):
        """Add an experience to the buffer for later training.
        
        Call this after each action is taken and a reward is received.
        """
        self.experience_buffer.add(features, action, reward, done)
    
    def train_step(self) -> Optional[dict]:
        """Perform a PPO training step if we have enough experiences.
        
        Returns:
            Training metrics if training occurred, None otherwise.
        """
        if len(self.experience_buffer) < self.config.batch_size:
            return None
        
        # Get batch from buffer
        batch = self.experience_buffer.get_batch(self.config.batch_size)
        
        # PPO update (simplified version)
        metrics = self._ppo_update(batch)
        
        return metrics
    
    def _ppo_update(self, batch: dict) -> dict:
        """Perform PPO policy update.
        
        This is a simplified implementation. A full production version would
        include advantage estimation, multiple update epochs, and more.
        """
        # Convert batch to tensors
        features = torch.FloatTensor(batch['features']).to(self.device)
        old_log_probs = torch.FloatTensor(batch['log_probs']).to(self.device)
        rewards = torch.FloatTensor(batch['rewards']).to(self.device)
        
        total_loss = 0.0
        
        for _ in range(self.config.update_epochs):
            # Forward pass
            cont_mean, cont_log_std, disc_logits, values = self.policy(features)
            
            # Compute new log probabilities
            cont_dist = torch.distributions.Normal(cont_mean, cont_log_std.exp())
            disc_dist = torch.distributions.Categorical(logits=disc_logits)
            
            # For simplicity, we're using rewards directly as advantages
            # A proper implementation would compute GAE advantages
            advantages = rewards - values.detach()
            
            # PPO clipped objective (simplified)
            # In practice, you'd compute the ratio and clip it properly
            policy_loss = -advantages.mean()
            value_loss = F.mse_loss(values, rewards)
            entropy = cont_dist.entropy().mean() + disc_dist.entropy().mean()
            
            loss = (
                policy_loss 
                + self.config.value_coef * value_loss 
                - self.config.entropy_coef * entropy
            )
            
            self.optimizer.zero_grad()
            loss.backward()
            torch.nn.utils.clip_grad_norm_(
                self.policy.parameters(), 
                self.config.max_grad_norm
            )
            self.optimizer.step()
            
            total_loss += loss.item()
        
        return {
            'loss': total_loss / self.config.update_epochs,
            'batch_size': len(batch['features']),
        }
    
    def save(self, path: str):
        """Save the decoder to a file."""
        torch.save({
            'config': self.config,
            'policy_state': self.policy.state_dict(),
            'optimizer_state': self.optimizer.state_dict(),
        }, path)
    
    @classmethod
    def load(cls, path: str) -> 'Decoder':
        """Load a decoder from a file."""
        checkpoint = torch.load(path)
        decoder = cls(checkpoint['config'])
        decoder.policy.load_state_dict(checkpoint['policy_state'])
        decoder.optimizer.load_state_dict(checkpoint['optimizer_state'])
        return decoder


class ExperienceBuffer:
    """Simple buffer for storing experiences before training.
    
    In a production system, you might want a more sophisticated replay buffer
    with prioritized sampling, but this simple version works for our use case.
    """
    
    def __init__(self, max_size: int = 10000):
        self.max_size = max_size
        self.features = []
        self.actions = []
        self.rewards = []
        self.log_probs = []
        self.dones = []
    
    def add(self, features: np.ndarray, action: dict, reward: float, done: bool):
        """Add an experience to the buffer."""
        self.features.append(features)
        self.actions.append(action)
        self.rewards.append(reward)
        self.dones.append(done)
        
        # Compute and store log_prob (simplified - in practice you'd store this
        # at action time)
        self.log_probs.append(0.0)
        
        # Maintain max size
        if len(self.features) > self.max_size:
            self.features.pop(0)
            self.actions.pop(0)
            self.rewards.pop(0)
            self.log_probs.pop(0)
            self.dones.pop(0)
    
    def __len__(self) -> int:
        return len(self.features)
    
    def get_batch(self, batch_size: int) -> dict:
        """Get a batch of experiences."""
        indices = np.random.choice(len(self.features), batch_size, replace=False)
        
        return {
            'features': np.array([self.features[i] for i in indices]),
            'rewards': np.array([self.rewards[i] for i in indices]),
            'log_probs': np.array([self.log_probs[i] for i in indices]),
        }
    
    def clear(self):
        """Clear the buffer."""
        self.features.clear()
        self.actions.clear()
        self.rewards.clear()
        self.log_probs.clear()
        self.dones.clear()
