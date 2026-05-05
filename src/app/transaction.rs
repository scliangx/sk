//! 事务管理模块
//!
//! 提供操作记录与回滚能力，确保关键操作（如 add --with-password）
//! 在任何步骤失败时能够安全回滚到操作前的状态。
//!
//! 回滚策略：
//! - 推送公钥失败 → 删除已生成的密钥文件
//! - 写入 config 失败 → 删除密钥文件 + 回滚远程公钥
//! - 保存密码失败 → 仅删除密码条目（标记为已完成的操作不回滚）

use crate::error::SkResult;

/// 回滚操作的定义
pub type RollbackAction = Box<dyn FnOnce() -> SkResult<()>>;

/// 事务管理器
///
/// 记录操作序列，在失败时按逆序执行回滚操作。
pub struct Transaction {
    /// 当前事务中的回滚操作栈（后进先出）
    rollback_stack: Vec<RollbackAction>,
    /// 事务是否已提交
    committed: bool,
}

impl Transaction {
    /// 开始一个新事务
    pub fn begin() -> Self {
        Self {
            rollback_stack: Vec::new(),
            committed: false,
        }
    }

    /// 注册一个回滚操作
    ///
    /// 回滚操作在事务中止时按注册的逆序执行。
    pub fn on_rollback<F>(&mut self, action: F)
    where
        F: FnOnce() -> SkResult<()> + 'static,
    {
        self.rollback_stack.push(Box::new(action));
    }

    /// 提交事务（标记为完成，不回滚）
    ///
    /// 提交后，drop 时不会执行回滚操作。
    pub fn commit(mut self) {
        self.committed = true;
        // 清理回滚栈（事务已成功完成）
        self.rollback_stack.clear();
    }

    /// 中止事务（执行回滚）
    ///
    /// 按注册的逆序执行所有回滚操作。
    /// 即使某个回滚操作失败，也会继续执行剩余的。
    pub fn rollback(&mut self) {
        // 逆序执行所有回滚操作
        while let Some(action) = self.rollback_stack.pop() {
            if let Err(e) = action() {
                // 回滚失败时记录警告但继续执行其他回滚
                eprintln!("Warning: rollback operation failed: {}", e);
            }
        }
        self.committed = true;
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        // 如果事务未提交且未显式回滚，自动执行回滚
        if !self.committed && !self.rollback_stack.is_empty() {
            self.rollback();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn test_transaction_commit() {
        let mut tx = Transaction::begin();
        let executed = Rc::new(RefCell::new(false));
        let executed_clone = executed.clone();

        tx.on_rollback(move || {
            *executed_clone.borrow_mut() = true;
            Ok(())
        });

        tx.commit();
        // 提交后回滚操作不应执行
        assert!(!*executed.borrow());
    }

    #[test]
    fn test_transaction_rollback() {
        let mut tx = Transaction::begin();
        let executed = Rc::new(RefCell::new(false));
        let executed_clone = executed.clone();

        tx.on_rollback(move || {
            *executed_clone.borrow_mut() = true;
            Ok(())
        });

        tx.rollback();
        // 回滚操作应该已执行
        assert!(*executed.borrow());
    }

    #[test]
    fn test_transaction_rollback_order() {
        let mut tx = Transaction::begin();
        let order = Rc::new(RefCell::new(Vec::new()));

        let o1 = order.clone();
        tx.on_rollback(move || {
            o1.borrow_mut().push(1);
            Ok(())
        });

        let o2 = order.clone();
        tx.on_rollback(move || {
            o2.borrow_mut().push(2);
            Ok(())
        });

        tx.rollback();
        // 逆序执行：先 2 后 1
        assert_eq!(*order.borrow(), vec![2, 1]);
    }

    #[test]
    fn test_transaction_drop_rollback() {
        let executed = Rc::new(RefCell::new(false));
        let executed_clone = executed.clone();

        {
            let mut tx = Transaction::begin();
            tx.on_rollback(move || {
                *executed_clone.borrow_mut() = true;
                Ok(())
            });
            // tx drop 时应自动回滚
        }

        assert!(*executed.borrow());
    }
}
